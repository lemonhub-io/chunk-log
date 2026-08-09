//! High-level repository operations.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};

use crate::object::{parse_hash, ChunkCoords, Commit, Hash, Object};
use crate::store::{FilesystemStore, ObjectStore};

const CHUNKLOG_DIR: &str = ".chunklog";
const HEAD_FILE: &str = ".chunklog/HEAD";
const OBJECTS_DIR: &str = ".chunklog/objects";
const STAGING_DIR: &str = ".chunklog/staging";

/// A world snapshot: chunk coordinates mapped to compressed chunk data.
///
/// This is the unit of input for [`Repository::commit`].
pub type World = HashMap<ChunkCoords, Vec<u8>>;

/// A single entry in a repository's commit history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// Hash of the commit.
    pub hash: Hash,
    /// Commit message.
    pub message: String,
}

/// A chunklog repository.
///
/// The repository layout (`.chunklog/`, `HEAD`, staging) lives on the
/// filesystem, while objects are stored in a pluggable
/// [`ObjectStore`]. [`Repository::init`] and [`Repository::open`] use
/// the filesystem backend; any other backend can be used via
/// [`Repository::init_with`] and [`Repository::open_with`].
pub struct Repository<S> {
    root: PathBuf,
    store: S,
    head: Option<Hash>,
}

impl Repository<FilesystemStore> {
    /// Creates a new repository at `path` with a filesystem object store.
    pub fn init(path: &Path) -> Result<Repository<FilesystemStore>> {
        Repository::init_with(FilesystemStore::new(path.join(OBJECTS_DIR)), path)
    }

    /// Opens an existing repository at `path` with a filesystem object store.
    pub fn open(path: &Path) -> Result<Repository<FilesystemStore>> {
        Repository::open_with(FilesystemStore::new(path.join(OBJECTS_DIR)), path)
    }
}

impl<S: ObjectStore> Repository<S> {
    /// Creates a new repository at `path`, storing objects in `store`.
    pub fn init_with(store: S, path: &Path) -> Result<Self> {
        let chunklog_dir = path.join(CHUNKLOG_DIR);
        if chunklog_dir.exists() {
            bail!("repository already exists at {}", path.display());
        }
        fs::create_dir_all(chunklog_dir.join("objects"))?;
        fs::create_dir_all(path.join(STAGING_DIR))?;
        fs::write(path.join(HEAD_FILE), b"")?;
        Ok(Repository {
            root: path.to_path_buf(),
            store,
            head: None,
        })
    }

    /// Opens an existing repository at `path`, reading objects from `store`.
    pub fn open_with(store: S, path: &Path) -> Result<Self> {
        let chunklog_dir = path.join(CHUNKLOG_DIR);
        if !chunklog_dir.is_dir() {
            bail!("not a chunklog repository at {}", path.display());
        }
        Ok(Repository {
            root: path.to_path_buf(),
            store,
            head: read_head(path)?,
        })
    }

    /// Creates a commit from `world`, updating HEAD.
    ///
    /// Blobs and trees are content-addressed, so unchanged chunks are
    /// deduplicated automatically.
    pub fn commit(&mut self, world: &World, message: &str) -> Result<Hash> {
        let mut tree = BTreeMap::new();
        for ((x, z), data) in world {
            let blob_hash = self.store.write(&Object::Blob(data.clone()).to_bytes())?;
            tree.insert((*x, *z), blob_hash);
        }
        let tree_hash = self.store.write(&Object::Tree(tree).to_bytes())?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before the unix epoch")?
            .as_secs();
        let commit = Commit {
            tree: tree_hash,
            parent: self.head,
            timestamp,
            message: message.to_string(),
        };
        let commit_hash = self.store.write(&Object::Commit(commit).to_bytes())?;
        fs::write(self.root.join(HEAD_FILE), commit_hash.to_string())?;
        self.head = Some(commit_hash);
        Ok(commit_hash)
    }

    /// Walks the commit history from HEAD, newest first.
    pub fn log(&self) -> Result<Vec<LogEntry>> {
        let mut entries = Vec::new();
        let mut current = self.head;
        while let Some(hash) = current {
            match self.read_object(hash)? {
                Object::Commit(commit) => {
                    entries.push(LogEntry {
                        hash,
                        message: commit.message,
                    });
                    current = commit.parent;
                }
                other => bail!("object {hash} is not a commit: {other:?}"),
            }
        }
        Ok(entries)
    }

    /// The current HEAD commit, if any.
    pub fn head(&self) -> Option<Hash> {
        self.head
    }

    /// The repository root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The object store backing this repository.
    pub fn store(&self) -> &S {
        &self.store
    }

    fn read_object(&self, hash: Hash) -> Result<Object> {
        Object::from_bytes(&self.store.read(hash)?).with_context(|| format!("corrupt object {hash}"))
    }
}

fn read_head(path: &Path) -> Result<Option<Hash>> {
    let contents = fs::read_to_string(path.join(HEAD_FILE)).context("failed to read HEAD")?;
    if contents.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(parse_hash(contents.trim())?))
}
