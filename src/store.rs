//! Storage backends.

use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use anyhow::{Context, Result};

use crate::object::{parse_hash, Hash};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A content-addressed key-value store mapping hashes to object bytes.
///
/// Repository objects, including blobs, are stored as typed canonical bytes
/// produced by [`Object::to_bytes`](crate::Object::to_bytes). `write`
/// derives the hash from the complete bytes, so identical objects are stored
/// only once.
///
/// Implementations must support enumerating and deleting objects so that
/// garbage collection can reclaim unreachable data.
pub trait ObjectStore {
    /// Reads the raw bytes stored under `hash`.
    ///
    /// Implementations must reject bytes whose content hash does not equal
    /// `hash`. [`Repository`](crate::Repository) verifies this invariant a
    /// second time so custom stores cannot silently corrupt repository data.
    fn read(&self, hash: Hash) -> Result<Vec<u8>>;

    /// Stores `data` under its content hash and returns that hash.
    fn write(&self, data: &[u8]) -> Result<Hash>;

    /// Lists all hashes currently stored.
    fn list(&self) -> Result<Vec<Hash>>;

    /// Deletes the object stored under `hash`. Deleting a missing object
    /// is a no-op.
    fn delete(&self, hash: Hash) -> Result<()>;
}

/// A verified in-memory object store for tests, simulations and
/// backend-independent algorithm benchmarks.
#[derive(Default)]
pub struct MemoryStore {
    objects: RwLock<HashMap<Hash, Vec<u8>>>,
}

impl MemoryStore {
    /// Creates an empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ObjectStore for MemoryStore {
    fn read(&self, hash: Hash) -> Result<Vec<u8>> {
        let bytes = self
            .objects
            .read()
            .expect("memory store lock poisoned")
            .get(&hash)
            .cloned()
            .with_context(|| format!("object {hash} not found"))?;
        hash.verify(&bytes)?;
        Ok(bytes)
    }

    fn write(&self, data: &[u8]) -> Result<Hash> {
        let hash = Hash::digest(data);
        let mut objects = self.objects.write().expect("memory store lock poisoned");
        if let Some(existing) = objects.get(&hash) {
            hash.verify(existing)?;
        } else {
            objects.insert(hash, data.to_vec());
        }
        Ok(hash)
    }

    fn list(&self) -> Result<Vec<Hash>> {
        Ok(self
            .objects
            .read()
            .expect("memory store lock poisoned")
            .keys()
            .copied()
            .collect())
    }

    fn delete(&self, hash: Hash) -> Result<()> {
        self.objects
            .write()
            .expect("memory store lock poisoned")
            .remove(&hash);
        Ok(())
    }
}

/// A filesystem-backed [`ObjectStore`].
///
/// Objects are stored as `objects/<hex-hash>` files. Data is written to a
/// unique temporary file before publication by rename. Reads always verify
/// that file contents match the requested address. The default backend does
/// not issue a power-loss durability barrier for every object; it guarantees
/// process-level atomic publication, not survival of sudden power failure.
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
        let bytes = fs::read(&path).with_context(|| format!("failed to read object {hash}"))?;
        hash.verify(&bytes)
            .with_context(|| format!("corrupt object file {}", path.display()))?;
        Ok(bytes)
    }

    fn write(&self, data: &[u8]) -> Result<Hash> {
        let hash = Hash::digest(data);
        let path = self.objects_dir.join(hash.to_string());
        if path.exists() {
            self.read(hash)?;
            return Ok(hash);
        }
        fs::create_dir_all(&self.objects_dir)?;
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let tmp = self
            .objects_dir
            .join(format!(".tmp-{}-{sequence}-{hash}", std::process::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .with_context(|| format!("failed to create temporary object {}", tmp.display()))?;
        if let Err(error) = (|| -> Result<()> {
            file.write_all(data)?;
            drop(file);
            fs::rename(&tmp, &path)?;
            Ok(())
        })() {
            let _ = fs::remove_file(&tmp);
            if path.exists() {
                self.read(hash)?;
                return Ok(hash);
            }
            return Err(error).with_context(|| format!("failed to store object {hash}"));
        }
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
