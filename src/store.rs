//! Storage backends.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::object::{parse_hash, Hash};

/// A content-addressed key-value store mapping hashes to object bytes.
///
/// Chunk blobs are stored as raw bytes; structured objects (trees,
/// commits) are stored serialized via
/// [`Object::to_bytes`](crate::Object::to_bytes). `write` derives the
/// hash from the data itself, so identical data is stored only once.
///
/// Implementations must support enumerating and deleting objects so that
/// garbage collection can reclaim unreachable data.
pub trait ObjectStore {
    /// Reads the raw bytes stored under `hash`.
    fn read(&self, hash: Hash) -> Result<Vec<u8>>;

    /// Stores `data` under its content hash and returns that hash.
    fn write(&self, data: &[u8]) -> Result<Hash>;

    /// Lists all hashes currently stored.
    fn list(&self) -> Result<Vec<Hash>>;

    /// Deletes the object stored under `hash`. Deleting a missing object
    /// is a no-op.
    fn delete(&self, hash: Hash) -> Result<()>;
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
        let hash = Hash(Sha256::digest(data).into());
        let path = self.objects_dir.join(hash.to_string());
        if path.exists() {
            return Ok(hash);
        }
        let tmp = self.objects_dir.join(format!(".tmp-{hash}"));
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &path).with_context(|| format!("failed to store object {hash}"))?;
        Ok(hash)
    }

    fn list(&self) -> Result<Vec<Hash>> {
        let mut hashes = Vec::new();
        for entry in fs::read_dir(&self.objects_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            hashes.push(
                parse_hash(&name)
                    .with_context(|| format!("unexpected file in object store: {name}"))?,
            );
        }
        Ok(hashes)
    }

    fn delete(&self, hash: Hash) -> Result<()> {
        let path = self.objects_dir.join(hash.to_string());
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
