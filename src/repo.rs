//! High-level repository operations.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};

use crate::diff::WorldDiff;
use crate::gc::GcStats;
use crate::object::{parse_hash, ChunkCoords, Commit, Hash, Object};
use crate::store::{FilesystemStore, ObjectStore};

const CHUNKLOG_DIR: &str = ".chunklog";
const HEAD_FILE: &str = ".chunklog/HEAD";
const OBJECTS_DIR: &str = ".chunklog/objects";
const REFS_HEADS_DIR: &str = ".chunklog/refs/heads";
const STAGING_DIR: &str = ".chunklog/staging";
const DEFAULT_BRANCH: &str = "main";

/// A world snapshot: chunk coordinates mapped to compressed chunk data.
///
/// This is the unit of input for [`Repository::commit`] and the result
/// of [`Repository::load`].
pub type World = HashMap<ChunkCoords, Vec<u8>>;

/// A single entry in a repository's commit history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// Hash of the commit.
    pub hash: Hash,
    /// Commit message.
    pub message: String,
}

/// A branch reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    /// Branch name.
    pub name: String,
    /// Commit the branch points to, or `None` if the branch is unborn.
    pub commit: Option<Hash>,
}

/// The result of a successful checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    /// Branch switched to, or `None` for a detached HEAD.
    pub branch: Option<String>,
    /// Commit now checked out.
    pub commit: Hash,
}

/// A chunklog repository.
///
/// The repository layout (`.chunklog/`, `HEAD`, refs, staging) lives on
/// the filesystem, while objects are stored in a pluggable [`ObjectStore`].
/// [`Repository::init`] and [`Repository::open`] use the filesystem
/// backend; any other backend can be used via [`Repository::init_with`]
/// and [`Repository::open_with`].
///
/// `HEAD` is a symbolic reference: on a branch it contains
/// `ref: refs/heads/<name>`, and in a detached state it contains a
/// commit hash directly.
pub struct Repository<S> {
    root: PathBuf,
    store: S,
    head: Option<Hash>,
    current_branch: Option<String>,
}

impl Repository<FilesystemStore> {
    /// Creates a new repository at `path` with a filesystem object store.
    ///
    /// The initial branch is `main`.
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
    ///
    /// The initial branch is `main`.
    pub fn init_with(store: S, path: &Path) -> Result<Self> {
        let chunklog_dir = path.join(CHUNKLOG_DIR);
        if chunklog_dir.exists() {
            bail!("repository already exists at {}", path.display());
        }
        fs::create_dir_all(chunklog_dir.join("objects"))?;
        fs::create_dir_all(chunklog_dir.join("refs/heads"))?;
        fs::create_dir_all(path.join(STAGING_DIR))?;
        fs::write(path.join(HEAD_FILE), format!("ref: refs/heads/{DEFAULT_BRANCH}"))?;
        Ok(Repository {
            root: path.to_path_buf(),
            store,
            head: None,
            current_branch: Some(DEFAULT_BRANCH.to_string()),
        })
    }

    /// Opens an existing repository at `path`, reading objects from `store`.
    pub fn open_with(store: S, path: &Path) -> Result<Self> {
        let chunklog_dir = path.join(CHUNKLOG_DIR);
        if !chunklog_dir.is_dir() {
            bail!("not a chunklog repository at {}", path.display());
        }
        let (current_branch, head) = read_head(path)?;
        Ok(Repository {
            root: path.to_path_buf(),
            store,
            head,
            current_branch,
        })
    }

    /// Creates a commit from `world`, updating the current branch (or HEAD
    /// when detached).
    ///
    /// Blobs and trees are content-addressed, so unchanged chunks are
    /// deduplicated automatically.
    pub fn commit(&mut self, world: &World, message: &str) -> Result<Hash> {
        let mut tree = BTreeMap::new();
        for ((x, z), data) in world {
            let blob_hash = self.store.write(data)?;
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
        match &self.current_branch {
            Some(name) => fs::write(self.ref_path(name), commit_hash.to_string())?,
            None => fs::write(self.root.join(HEAD_FILE), commit_hash.to_string())?,
        }
        self.head = Some(commit_hash);
        Ok(commit_hash)
    }

    /// Loads the full world of a commit: all chunk blobs under its tree.
    ///
    /// Blobs hold compressed chunk data as stored; decompression is left
    /// to the game.
    pub fn load(&self, commit: Hash) -> Result<World> {
        let mut world = World::new();
        for (coords, blob_hash) in self.chunk_hashes(commit)? {
            world.insert(coords, self.store.read(blob_hash)?);
        }
        Ok(world)
    }

    /// Lists `(coordinates, blob hash)` pairs of a commit's tree, sorted
    /// by coordinates. Useful for loading chunks on demand.
    pub fn chunk_hashes(&self, commit: Hash) -> Result<Vec<(ChunkCoords, Hash)>> {
        Ok(self.tree_entries(commit)?.into_iter().collect())
    }

    /// Computes the difference between the worlds of two commits.
    ///
    /// `from` may be `None` to diff against an empty world. Results are
    /// sorted by coordinates.
    pub fn diff(&self, from: Option<Hash>, to: Hash) -> Result<WorldDiff> {
        let from_tree = match from {
            Some(hash) => self.tree_entries(hash)?,
            None => BTreeMap::new(),
        };
        let to_tree = self.tree_entries(to)?;

        let mut added = Vec::new();
        let mut modified = Vec::new();
        for (coords, hash) in &to_tree {
            match from_tree.get(coords) {
                None => added.push((*coords, *hash)),
                Some(old) if old != hash => modified.push((*coords, (*old, *hash))),
                _ => {}
            }
        }
        let mut removed = Vec::new();
        for (coords, hash) in &from_tree {
            if !to_tree.contains_key(coords) {
                removed.push((*coords, *hash));
            }
        }
        Ok(WorldDiff {
            added,
            modified,
            removed,
        })
    }

    /// Resolves a branch name or commit hash to a commit hash.
    pub fn resolve(&self, target: &str) -> Result<Hash> {
        if let Some(commit) = self.read_ref(target)? {
            return Ok(commit);
        }
        let hash =
            parse_hash(target).with_context(|| format!("no such branch or commit: {target}"))?;
        match self.read_object(hash)? {
            Object::Commit(_) => Ok(hash),
            other => bail!("object {hash} is not a commit: {other:?}"),
        }
    }

    /// Deletes all objects unreachable from HEAD and any branch ref.
    ///
    /// Traversal follows commits to their parent chains and trees to
    /// their blobs. Fails loudly rather than deleting anything when the
    /// store is corrupt.
    pub fn collect_garbage(&self) -> Result<GcStats> {
        let mut reachable = HashSet::new();
        let mut commits: Vec<Hash> = Vec::new();
        let mut trees: Vec<Hash> = Vec::new();
        if let Some(hash) = self.head {
            commits.push(hash);
        }
        for branch in self.branches()? {
            if let Some(hash) = branch.commit {
                commits.push(hash);
            }
        }
        while let Some(hash) = commits.pop() {
            if !reachable.insert(hash) {
                continue;
            }
            let Object::Commit(commit) = self.read_object(hash)? else {
                bail!("object {hash} is not a commit");
            };
            trees.push(commit.tree);
            if let Some(parent) = commit.parent {
                commits.push(parent);
            }
        }
        while let Some(hash) = trees.pop() {
            if !reachable.insert(hash) {
                continue;
            }
            let Object::Tree(entries) = self.read_object(hash)? else {
                bail!("object {hash} is not a tree");
            };
            reachable.extend(entries.values().copied());
        }

        let all = self.store.list()?;
        let mut removed = 0;
        for hash in all {
            if !reachable.contains(&hash) {
                self.store.delete(hash)?;
                removed += 1;
            }
        }
        let retained = self.store.list()?.len();
        Ok(GcStats { removed, retained })
    }

    /// Creates a branch at the current HEAD.
    pub fn create_branch(&mut self, name: &str) -> Result<()> {
        validate_branch_name(name)?;
        let path = self.ref_path(name);
        if path.exists() {
            bail!("branch '{name}' already exists");
        }
        let content = self.head.map(|h| h.to_string()).unwrap_or_default();
        fs::write(path, content)?;
        Ok(())
    }

    /// Deletes a branch. The current branch cannot be deleted.
    pub fn delete_branch(&mut self, name: &str) -> Result<()> {
        if self.current_branch.as_deref() == Some(name) {
            bail!("cannot delete the current branch '{name}'");
        }
        let path = self.ref_path(name);
        if !path.exists() {
            bail!("branch '{name}' does not exist");
        }
        fs::remove_file(path)?;
        Ok(())
    }

    /// Lists all branches, sorted by name.
    pub fn branches(&self) -> Result<Vec<Branch>> {
        let dir = self.root.join(REFS_HEADS_DIR);
        let mut branches = Vec::new();
        if dir.is_dir() {
            for entry in fs::read_dir(&dir)? {
                let entry = entry?;
                if !entry.file_type()?.is_file() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                branches.push(Branch {
                    name: name.clone(),
                    commit: self.read_ref(&name)?,
                });
            }
        }
        branches.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(branches)
    }

    /// The current branch, or `None` when HEAD is detached.
    pub fn current_branch(&self) -> Option<&str> {
        self.current_branch.as_deref()
    }

    /// Switches to `target`, which may be a branch name or a commit hash.
    ///
    /// A branch name checks out that branch; a commit hash checks out a
    /// detached HEAD. Only references are moved — world data is available
    /// via [`Repository::load`] and [`Repository::chunk_hashes`].
    pub fn checkout(&mut self, target: &str) -> Result<Checkout> {
        if let Some(commit) = self.read_ref(target)? {
            return self.switch_to_branch(target, commit);
        }
        if self.current_branch.as_deref() == Some(target) {
            bail!("cannot checkout '{target}': branch has no commits yet");
        }
        let hash = parse_hash(target)
            .with_context(|| format!("cannot checkout '{target}': no such branch or commit"))?;
        match self.read_object(hash)? {
            Object::Commit(_) => self.switch_to_detached(hash),
            other => bail!("object {hash} is not a commit: {other:?}"),
        }
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

    fn switch_to_branch(&mut self, name: &str, commit: Hash) -> Result<Checkout> {
        self.current_branch = Some(name.to_string());
        self.head = Some(commit);
        self.write_head()?;
        Ok(Checkout {
            branch: Some(name.to_string()),
            commit,
        })
    }

    fn switch_to_detached(&mut self, commit: Hash) -> Result<Checkout> {
        self.current_branch = None;
        self.head = Some(commit);
        self.write_head()?;
        Ok(Checkout {
            branch: None,
            commit,
        })
    }

    fn write_head(&self) -> Result<()> {
        let content = match &self.current_branch {
            Some(name) => format!("ref: refs/heads/{name}"),
            None => self
                .head
                .expect("detached HEAD always has a commit")
                .to_string(),
        };
        fs::write(self.root.join(HEAD_FILE), content)?;
        Ok(())
    }

    fn ref_path(&self, name: &str) -> PathBuf {
        self.root.join(REFS_HEADS_DIR).join(name)
    }

    fn read_ref(&self, name: &str) -> Result<Option<Hash>> {
        read_branch_ref(&self.root, name)
    }

    fn tree_entries(&self, commit: Hash) -> Result<BTreeMap<ChunkCoords, Hash>> {
        let Object::Commit(commit_obj) = self.read_object(commit)? else {
            bail!("object {commit} is not a commit");
        };
        let Object::Tree(entries) = self.read_object(commit_obj.tree)? else {
            bail!("object {} is not a tree", commit_obj.tree);
        };
        Ok(entries)
    }

    fn read_object(&self, hash: Hash) -> Result<Object> {
        Object::from_bytes(&self.store.read(hash)?).with_context(|| format!("corrupt object {hash}"))
    }
}

fn read_head(path: &Path) -> Result<(Option<String>, Option<Hash>)> {
    let contents = fs::read_to_string(path.join(HEAD_FILE)).context("failed to read HEAD")?;
    let trimmed = contents.trim();
    if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
        let branch = branch.trim();
        if branch.is_empty() || branch.contains('/') || branch.contains('\\') {
            bail!("corrupt HEAD: {trimmed}");
        }
        return Ok((Some(branch.to_string()), read_branch_ref(path, branch)?));
    }
    if trimmed.is_empty() {
        bail!("corrupt HEAD: empty");
    }
    let hash = parse_hash(trimmed).with_context(|| format!("corrupt HEAD: {trimmed}"))?;
    Ok((None, Some(hash)))
}

fn read_branch_ref(root: &Path, name: &str) -> Result<Option<Hash>> {
    let path = root.join(REFS_HEADS_DIR).join(name);
    match fs::read_to_string(path) {
        Ok(contents) => {
            let trimmed = contents.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(parse_hash(trimmed)?))
            }
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn validate_branch_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("branch name cannot be empty");
    }
    if name.contains('/')
        || name.contains('\\')
        || name.contains(char::is_whitespace)
        || name.starts_with('.')
        || name.ends_with('.')
        || name.ends_with(' ')
    {
        bail!("invalid branch name: {name}");
    }
    Ok(())
}
