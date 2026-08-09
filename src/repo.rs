use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};

use crate::object::{parse_hash, Commit, Hash, Object};
use crate::store::{FilesystemStore, ObjectStore};

const CHUNKLOG_DIR: &str = ".chunklog";
const HEAD_FILE: &str = ".chunklog/HEAD";
const OBJECTS_DIR: &str = ".chunklog/objects";
const STAGING_DIR: &str = ".chunklog/staging";

/// High-level interface to a chunklog repository.
pub struct Repository {
    root: PathBuf,
    store: FilesystemStore,
    head: Option<Hash>,
}

impl Repository {
    /// Creates a new repository at `path`.
    pub fn init(path: &Path) -> Result<Repository> {
        let chunklog_dir = path.join(CHUNKLOG_DIR);
        if chunklog_dir.exists() {
            bail!("repository already exists at {}", path.display());
        }
        fs::create_dir_all(chunklog_dir.join("objects"))?;
        fs::create_dir_all(path.join(STAGING_DIR))?;
        fs::write(path.join(HEAD_FILE), b"")?;
        Ok(Repository {
            root: path.to_path_buf(),
            store: FilesystemStore::new(path.join(OBJECTS_DIR)),
            head: None,
        })
    }

    /// Opens an existing repository at `path`.
    pub fn open(path: &Path) -> Result<Repository> {
        let chunklog_dir = path.join(CHUNKLOG_DIR);
        if !chunklog_dir.is_dir() {
            bail!("not a chunklog repository at {}", path.display());
        }
        Ok(Repository {
            root: path.to_path_buf(),
            store: FilesystemStore::new(path.join(OBJECTS_DIR)),
            head: read_head(path)?,
        })
    }

    /// Creates a commit from `chunks`, updating HEAD.
    pub fn commit(
        &mut self,
        chunks: &HashMap<(i32, i32), Vec<u8>>,
        message: &str,
    ) -> Result<Hash> {
        let mut tree = BTreeMap::new();
        for ((x, z), data) in chunks {
            let blob_hash = self.store.write(&Object::Blob(data.clone()))?;
            tree.insert((*x, *z), blob_hash);
        }
        let tree_hash = self.store.write(&Object::Tree(tree))?;
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
        let commit_hash = self.store.write(&Object::Commit(commit))?;
        fs::write(self.root.join(HEAD_FILE), commit_hash.to_string())?;
        self.head = Some(commit_hash);
        Ok(commit_hash)
    }

    /// Walks the commit history from HEAD, newest first.
    pub fn log(&self) -> Result<Vec<(Hash, String)>> {
        let mut entries = Vec::new();
        let mut current = self.head;
        while let Some(hash) = current {
            match self.store.read(hash)? {
                Object::Commit(commit) => {
                    entries.push((hash, commit.message));
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

    /// The underlying object store.
    pub fn store(&self) -> &FilesystemStore {
        &self.store
    }
}

fn read_head(path: &Path) -> Result<Option<Hash>> {
    let contents = fs::read_to_string(path.join(HEAD_FILE)).context("failed to read HEAD")?;
    if contents.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(parse_hash(contents.trim())?))
}
