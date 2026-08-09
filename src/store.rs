//! Storage backends.

use std::collections::HashMap;
use std::fs;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, RwLock};

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

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

    /// Starts an object-write batch.
    ///
    /// Stores without transactional batching may keep the default no-op.
    fn begin_batch(&self) -> Result<()> {
        Ok(())
    }

    /// Makes the current object-write batch visible and durable according to
    /// the backend's configured durability mode.
    fn commit_batch(&self) -> Result<()> {
        Ok(())
    }

    /// Discards the current object-write batch. This must be safe to call
    /// after an operation fails, including when no batch is active.
    fn rollback_batch(&self) -> Result<()> {
        Ok(())
    }
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

/// A verified SQLite-backed content-addressed object store.
///
/// All objects share one database instead of creating one filesystem entry per
/// Merkle node. Repository commits use [`ObjectStore::begin_batch`] and
/// [`ObjectStore::commit_batch`] so thousands of inserts require one SQLite
/// transaction. The database uses rollback journaling and `synchronous=FULL`;
/// object rows are committed before repository refs are published.
pub struct SqliteStore {
    connection: Mutex<Connection>,
}

impl SqliteStore {
    /// Opens or creates a SQLite object database at `path`.
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)
            .with_context(|| format!("failed to open SQLite object store {}", path.display()))?;
        connection.busy_timeout(std::time::Duration::from_secs(30))?;
        connection.execute_batch(
            "PRAGMA journal_mode=DELETE;
             PRAGMA synchronous=FULL;
             PRAGMA temp_store=MEMORY;
             CREATE TABLE IF NOT EXISTS objects (
                 hash BLOB PRIMARY KEY NOT NULL CHECK(length(hash) = 32),
                 data BLOB NOT NULL
             ) WITHOUT ROWID;",
        )?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| anyhow::anyhow!("SQLite object-store lock poisoned"))
    }
}

impl ObjectStore for SqliteStore {
    fn read(&self, hash: Hash) -> Result<Vec<u8>> {
        let connection = self.connection()?;
        let bytes = connection
            .query_row(
                "SELECT data FROM objects WHERE hash = ?1",
                params![&hash.0[..]],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .with_context(|| format!("object {hash} not found"))?;
        hash.verify(&bytes)
            .with_context(|| format!("corrupt SQLite object {hash}"))?;
        Ok(bytes)
    }

    fn write(&self, data: &[u8]) -> Result<Hash> {
        let hash = Hash::digest(data);
        let connection = self.connection()?;
        let inserted = connection.execute(
            "INSERT OR IGNORE INTO objects(hash, data) VALUES (?1, ?2)",
            params![&hash.0[..], data],
        )?;
        if inserted == 0 {
            let existing: Vec<u8> = connection.query_row(
                "SELECT data FROM objects WHERE hash = ?1",
                params![&hash.0[..]],
                |row| row.get(0),
            )?;
            hash.verify(&existing)?;
            if existing != data {
                bail!("hash collision while writing object {hash}");
            }
        }
        Ok(hash)
    }

    fn list(&self) -> Result<Vec<Hash>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT hash FROM objects")?;
        let rows = statement.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
        let mut hashes = Vec::new();
        for row in rows {
            let bytes = row?;
            let array: [u8; 32] = bytes
                .try_into()
                .map_err(|bytes: Vec<u8>| anyhow::anyhow!("invalid hash length {}", bytes.len()))?;
            hashes.push(Hash(array));
        }
        Ok(hashes)
    }

    fn delete(&self, hash: Hash) -> Result<()> {
        self.connection()?
            .execute("DELETE FROM objects WHERE hash = ?1", params![&hash.0[..]])?;
        Ok(())
    }

    fn begin_batch(&self) -> Result<()> {
        let connection = self.connection()?;
        if !connection.is_autocommit() {
            bail!("SQLite object batch is already active");
        }
        connection.execute_batch("BEGIN IMMEDIATE")?;
        Ok(())
    }

    fn commit_batch(&self) -> Result<()> {
        let connection = self.connection()?;
        if connection.is_autocommit() {
            bail!("no SQLite object batch is active");
        }
        connection.execute_batch("COMMIT")?;
        Ok(())
    }

    fn rollback_batch(&self) -> Result<()> {
        let connection = self.connection()?;
        if !connection.is_autocommit() {
            connection.execute_batch("ROLLBACK")?;
        }
        Ok(())
    }
}
