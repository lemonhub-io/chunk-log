use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::object::{Hash, Object};

/// A key-value store mapping content hashes to objects.
pub trait ObjectStore {
    /// Reads the object stored under `hash`.
    fn read(&self, hash: Hash) -> Result<Object>;

    /// Writes `object`, returning its hash. Existing objects are not
    /// rewritten (deduplication is inherent in content addressing).
    fn write(&self, object: &Object) -> Result<Hash>;
}

/// A filesystem-backed object store. Objects are stored as
/// `objects/<hex-hash>` files, written atomically via temp file + rename.
pub struct FilesystemStore {
    objects_dir: PathBuf,
}

impl FilesystemStore {
    pub fn new(objects_dir: impl Into<PathBuf>) -> Self {
        Self {
            objects_dir: objects_dir.into(),
        }
    }
}

impl ObjectStore for FilesystemStore {
    fn read(&self, hash: Hash) -> Result<Object> {
        let path = self.objects_dir.join(hash.to_string());
        let bytes =
            fs::read(&path).with_context(|| format!("failed to read object {hash}"))?;
        Object::from_bytes(&bytes).with_context(|| format!("corrupt object {hash}"))
    }

    fn write(&self, object: &Object) -> Result<Hash> {
        let hash = object.hash();
        let path = self.objects_dir.join(hash.to_string());
        if path.exists() {
            return Ok(hash);
        }
        let tmp = self.objects_dir.join(format!(".tmp-{hash}"));
        fs::write(&tmp, object.to_bytes())?;
        fs::rename(&tmp, &path).with_context(|| format!("failed to store object {hash}"))?;
        Ok(hash)
    }
}
