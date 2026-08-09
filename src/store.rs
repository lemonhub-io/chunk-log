//! Storage backends.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use xxhash_rust::xxh3::xxh3_64;

use crate::object::Hash;

/// A content-addressed key-value store mapping hashes to object bytes.
///
/// Chunk blobs are stored as raw bytes; structured objects (trees,
/// commits) are stored serialized via
/// [`Object::to_bytes`](crate::Object::to_bytes). `write` derives the
/// hash from the data itself, so identical data is stored only once.
pub trait ObjectStore {
    /// Reads the raw bytes stored under `hash`.
    fn read(&self, hash: Hash) -> Result<Vec<u8>>;

    /// Stores `data` under its content hash and returns that hash.
    fn write(&self, data: &[u8]) -> Result<Hash>;
}

/// A filesystem-backed [`ObjectStore`].
///
/// Objects are stored as `objects/<hex-hash>` files. Writes are atomic:
/// data goes to a temp file first and is renamed into place, so a crash
/// never leaves a partially written object.
pub struct FilesystemStore {
    objects_dir: PathBuf,
}

impl FilesystemStore {
    /// Creates a store rooted at `objects_dir`.
    pub fn new(objects_dir: impl Into<PathBuf>) -> Self {
        Self {
            objects_dir: objects_dir.into(),
        }
    }
}

impl ObjectStore for FilesystemStore {
    fn read(&self, hash: Hash) -> Result<Vec<u8>> {
        let path = self.objects_dir.join(hash.to_string());
        fs::read(&path).with_context(|| format!("failed to read object {hash}"))
    }

    fn write(&self, data: &[u8]) -> Result<Hash> {
        let hash = Hash(xxh3_64(data).to_le_bytes());
        let path = self.objects_dir.join(hash.to_string());
        if path.exists() {
            return Ok(hash);
        }
        let tmp = self.objects_dir.join(format!(".tmp-{hash}"));
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &path).with_context(|| format!("failed to store object {hash}"))?;
        Ok(hash)
    }
}
