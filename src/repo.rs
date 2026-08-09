//! High-level repository operations.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use atomicwrites::{AllowOverwrite, AtomicFile};

use crate::diff::WorldDiff;
use crate::gc::GcStats;
use crate::object::{parse_hash, ChunkCoords, Commit, Hash, Object, TreeNode};
use crate::store::{FilesystemStore, ObjectStore, SqliteStore};

const CHUNKLOG_DIR: &str = ".chunklog";
const HEAD_FILE: &str = ".chunklog/HEAD";
const FORMAT_FILE: &str = ".chunklog/FORMAT";
const OBJECTS_DIR: &str = ".chunklog/objects";
const OBJECTS_DB: &str = ".chunklog/objects.sqlite3";
const REFS_HEADS_DIR: &str = ".chunklog/refs/heads";
const STAGING_DIR: &str = ".chunklog/staging";
const LOCK_FILE: &str = ".chunklog/LOCK";
const DEFAULT_BRANCH: &str = "main";
const REPOSITORY_FORMAT: &str = "2\n";
const TREE_DEPTH: usize = 16;

/// A world snapshot: chunk coordinates mapped to opaque chunk data.
pub type World = HashMap<ChunkCoords, Vec<u8>>;

/// An incremental set of world changes.
///
/// Coordinates in `upserts` are inserted or replaced. Coordinates in
/// `removals` are deleted. A coordinate may not occur in both collections.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeSet {
    /// New or replacement chunk payloads.
    pub upserts: World,
    /// Coordinates to remove from the parent version.
    pub removals: HashSet<ChunkCoords>,
}

impl ChangeSet {
    /// Creates an empty change set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces one chunk.
    pub fn upsert(&mut self, coords: ChunkCoords, payload: Vec<u8>) {
        self.removals.remove(&coords);
        self.upserts.insert(coords, payload);
    }

    /// Removes one chunk.
    pub fn remove(&mut self, coords: ChunkCoords) {
        self.upserts.remove(&coords);
        self.removals.insert(coords);
    }

    /// Returns the number of affected coordinates.
    pub fn len(&self) -> usize {
        self.upserts.len() + self.removals.len()
    }

    /// Returns whether this change set affects no coordinates.
    pub fn is_empty(&self) -> bool {
        self.upserts.is_empty() && self.removals.is_empty()
    }
}

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
/// Repository metadata lives under `.chunklog/`; immutable objects live in
/// the supplied [`ObjectStore`]. Mutating operations take a repository lock,
/// and references are replaced atomically.
pub struct Repository<S> {
    root: PathBuf,
    store: S,
    head: Option<Hash>,
    current_branch: Option<String>,
}

impl Repository<SqliteStore> {
    /// Creates a repository using the transactional SQLite object store.
    pub fn init(path: &Path) -> Result<Self> {
        initialize_repository(path)?;
        let store = SqliteStore::new(path.join(OBJECTS_DB))?;
        Ok(Self {
            root: path.to_path_buf(),
            store,
            head: None,
            current_branch: Some(DEFAULT_BRANCH.to_string()),
        })
    }

    /// Opens a repository using the transactional SQLite object store.
    pub fn open(path: &Path) -> Result<Self> {
        validate_repository(path)?;
        let store = SqliteStore::new(path.join(OBJECTS_DB))?;
        let (current_branch, head) = read_head(path)?;
        Ok(Self {
            root: path.to_path_buf(),
            store,
            head,
            current_branch,
        })
    }
}

impl Repository<FilesystemStore> {
    /// Creates a format-2 repository using the legacy loose-file object store.
    pub fn init_loose(path: &Path) -> Result<Self> {
        Self::init_with(FilesystemStore::new(path.join(OBJECTS_DIR)), path)
    }

    /// Opens a format-2 repository using the legacy loose-file object store.
    pub fn open_loose(path: &Path) -> Result<Self> {
        Self::open_with(FilesystemStore::new(path.join(OBJECTS_DIR)), path)
    }
}

impl<S: ObjectStore> Repository<S> {
    /// Creates a repository using a custom object store.
    pub fn init_with(store: S, path: &Path) -> Result<Self> {
        initialize_repository(path)?;
        Ok(Self {
            root: path.to_path_buf(),
            store,
            head: None,
            current_branch: Some(DEFAULT_BRANCH.to_string()),
        })
    }

    /// Opens an existing repository using a custom object store.
    pub fn open_with(store: S, path: &Path) -> Result<Self> {
        validate_repository(path)?;
        let (current_branch, head) = read_head(path)?;
        Ok(Self {
            root: path.to_path_buf(),
            store,
            head,
            current_branch,
        })
    }

    /// Commits a complete world snapshot.
    ///
    /// This compatibility alias has Θ(N) payload scanning cost. Prefer
    /// [`commit_changes`](Self::commit_changes) when the caller already knows
    /// the edited coordinates.
    pub fn commit(&mut self, world: &World, message: &str) -> Result<Hash> {
        self.commit_snapshot(world, message)
    }

    /// Commits a complete world snapshot.
    pub fn commit_snapshot(&mut self, world: &World, message: &str) -> Result<Hash> {
        let _lock = self.lock()?;
        self.refresh_head()?;
        self.store.begin_batch()?;
        let result = (|| {
            let mut changes = BTreeMap::new();
            for (&coords, payload) in world {
                let blob = self.write_object(&Object::Blob(payload.clone()))?;
                changes.insert(coords, Some(blob));
            }
            let root = match self.apply_tree_changes(None, 0, &changes)? {
                Some(root) => root,
                None => self.empty_tree()?,
            };
            self.write_commit(root, message)
        })();
        let hash = self.finish_object_batch(result)?;
        self.publish_commit(hash)?;
        Ok(hash)
    }

    /// Applies an incremental change set to HEAD and creates a commit.
    ///
    /// Only affected radix-tree paths are republished. With fixed 64-bit
    /// coordinates the tree depth is sixteen, so structural work is bounded
    /// by the union of at most `16 * k` paths for `k` changed coordinates.
    pub fn commit_changes(&mut self, changes: &ChangeSet, message: &str) -> Result<Hash> {
        for coords in changes.upserts.keys() {
            if changes.removals.contains(coords) {
                bail!("coordinate {coords:?} is both upserted and removed");
            }
        }
        let _lock = self.lock()?;
        self.refresh_head()?;
        self.store.begin_batch()?;
        let result = (|| {
            let base_root = match self.head {
                Some(head) => self.read_commit(head)?.tree,
                None => self.empty_tree()?,
            };
            let mut encoded = BTreeMap::new();
            for (&coords, payload) in &changes.upserts {
                let blob = self.write_object(&Object::Blob(payload.clone()))?;
                encoded.insert(coords, Some(blob));
            }
            for &coords in &changes.removals {
                encoded.insert(coords, None);
            }
            let root = if encoded.is_empty() {
                base_root
            } else {
                match self.apply_tree_changes(Some(base_root), 0, &encoded)? {
                    Some(root) => root,
                    None => self.empty_tree()?,
                }
            };
            self.write_commit(root, message)
        })();
        let hash = self.finish_object_batch(result)?;
        self.publish_commit(hash)?;
        Ok(hash)
    }

    /// Loads the complete world represented by `commit`.
    pub fn load(&self, commit: Hash) -> Result<World> {
        let mut world = World::new();
        for (coords, blob) in self.chunk_hashes(commit)? {
            world.insert(coords, self.read_chunk(blob)?);
        }
        Ok(world)
    }

    /// Reads and verifies one chunk payload by blob address.
    pub fn read_chunk(&self, blob: Hash) -> Result<Vec<u8>> {
        match self.read_object(blob)? {
            Object::Blob(payload) => Ok(payload),
            other => bail!("object {blob} is not a blob: {other:?}"),
        }
    }

    /// Lists `(coordinate, blob hash)` pairs in canonical coordinate order.
    pub fn chunk_hashes(&self, commit: Hash) -> Result<Vec<(ChunkCoords, Hash)>> {
        let root = self.read_commit(commit)?.tree;
        Ok(self.tree_entries(root)?.into_iter().collect())
    }

    /// Computes the difference between two committed worlds.
    pub fn diff(&self, from: Option<Hash>, to: Hash) -> Result<WorldDiff> {
        let from_tree = match from {
            Some(hash) => self.tree_entries(self.read_commit(hash)?.tree)?,
            None => BTreeMap::new(),
        };
        let to_tree = self.tree_entries(self.read_commit(to)?.tree)?;
        let mut added = Vec::new();
        let mut modified = Vec::new();
        for (coords, hash) in &to_tree {
            match from_tree.get(coords) {
                None => added.push((*coords, *hash)),
                Some(old) if old != hash => modified.push((*coords, (*old, *hash))),
                _ => {}
            }
        }
        let removed = from_tree
            .iter()
            .filter(|(coords, _)| !to_tree.contains_key(coords))
            .map(|(coords, hash)| (*coords, *hash))
            .collect();
        Ok(WorldDiff {
            added,
            modified,
            removed,
        })
    }

    /// Resolves a validated branch name or a full commit hash.
    pub fn resolve(&self, target: &str) -> Result<Hash> {
        if validate_branch_name(target).is_ok() {
            let path = self.ref_path(target)?;
            if path.is_file() {
                return read_branch_ref_path(&path)?.with_context(|| {
                    format!("cannot resolve '{target}': branch has no commits yet")
                });
            }
        }
        let hash =
            parse_hash(target).with_context(|| format!("no such branch or commit: {target}"))?;
        self.read_commit(hash)?;
        Ok(hash)
    }

    /// Deletes every object unreachable from HEAD and all branch refs.
    ///
    /// Marking verifies every reachable object before sweep starts. Sweep is
    /// idempotent and safe to retry, but a storage failure can leave a prefix
    /// of unreachable objects deleted; this operation is not transactional.
    pub fn collect_garbage(&self) -> Result<GcStats> {
        let _lock = self.lock()?;
        let mut reachable = HashSet::new();
        let mut commits = Vec::new();
        let (_, current_head) = read_head(&self.root)?;
        if let Some(hash) = current_head {
            commits.push(hash);
        }
        for branch in self.branches()? {
            if let Some(hash) = branch.commit {
                commits.push(hash);
            }
        }
        let mut trees = Vec::new();
        while let Some(hash) = commits.pop() {
            if !reachable.insert(hash) {
                continue;
            }
            let commit = self.read_commit(hash)?;
            trees.push((commit.tree, 0usize, 0u64));
            if let Some(parent) = commit.parent {
                commits.push(parent);
            }
        }
        while let Some((hash, depth, prefix)) = trees.pop() {
            if !reachable.insert(hash) {
                continue;
            }
            match self.read_object(hash)? {
                Object::Tree(TreeNode::Branch(children)) if depth < TREE_DEPTH => {
                    for (nibble, child) in children {
                        let child_prefix = prefix | ((nibble as u64) << ((15 - depth) * 4));
                        trees.push((child, depth + 1, child_prefix));
                    }
                }
                Object::Tree(TreeNode::Leaf { coords, blob }) if depth == TREE_DEPTH => {
                    if coord_key(coords) != prefix {
                        bail!("tree leaf {hash} is stored under the wrong radix path");
                    }
                    if reachable.insert(blob) {
                        match self.read_object(blob)? {
                            Object::Blob(_) => {}
                            other => bail!("object {blob} is not a blob: {other:?}"),
                        }
                    }
                }
                other => bail!("invalid tree object {hash} at depth {depth}: {other:?}"),
            }
        }
        self.store.begin_batch()?;
        let sweep = (|| {
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
        })();
        let stats = match sweep {
            Ok(stats) => {
                if let Err(error) = self.store.commit_batch() {
                    let _ = self.store.rollback_batch();
                    return Err(error).context("failed to commit garbage-collection batch");
                }
                stats
            }
            Err(error) => {
                self.store
                    .rollback_batch()
                    .context("failed to roll back garbage-collection batch")?;
                return Err(error);
            }
        };
        Ok(stats)
    }

    /// Creates a branch at the current HEAD.
    pub fn create_branch(&mut self, name: &str) -> Result<()> {
        validate_branch_name(name)?;
        let _lock = self.lock()?;
        self.refresh_head()?;
        let path = self.ref_path(name)?;
        if path.exists() {
            bail!("branch '{name}' already exists");
        }
        let content = self.head.map(|hash| hash.to_string()).unwrap_or_default();
        atomic_write(&path, content.as_bytes())
    }

    /// Deletes a branch. The current branch cannot be deleted.
    pub fn delete_branch(&mut self, name: &str) -> Result<()> {
        validate_branch_name(name)?;
        let _lock = self.lock()?;
        self.refresh_head()?;
        if self.current_branch.as_deref() == Some(name) {
            bail!("cannot delete the current branch '{name}'");
        }
        let path = self.ref_path(name)?;
        if !path.is_file() {
            bail!("branch '{name}' does not exist");
        }
        fs::remove_file(path)?;
        Ok(())
    }

    /// Lists all branches in name order.
    pub fn branches(&self) -> Result<Vec<Branch>> {
        let dir = self.root.join(REFS_HEADS_DIR);
        let mut branches = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow!("branch name is not valid UTF-8"))?;
            validate_branch_name(&name)
                .with_context(|| format!("invalid branch ref file '{name}'"))?;
            branches.push(Branch {
                name,
                commit: read_branch_ref_path(&entry.path())?,
            });
        }
        branches.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(branches)
    }

    /// Returns the current branch, or `None` for detached HEAD.
    pub fn current_branch(&self) -> Option<&str> {
        self.current_branch.as_deref()
    }

    /// Switches HEAD to a branch or commit without materializing world data.
    pub fn checkout(&mut self, target: &str) -> Result<Checkout> {
        let _lock = self.lock()?;
        self.refresh_head()?;
        if validate_branch_name(target).is_ok() {
            let path = self.ref_path(target)?;
            if path.is_file() {
                let commit = read_branch_ref_path(&path)?.with_context(|| {
                    format!("cannot checkout '{target}': branch has no commits yet")
                })?;
                self.read_commit(commit)?;
                atomic_write(
                    &self.root.join(HEAD_FILE),
                    format!("ref: refs/heads/{target}").as_bytes(),
                )?;
                self.current_branch = Some(target.to_string());
                self.head = Some(commit);
                return Ok(Checkout {
                    branch: Some(target.to_string()),
                    commit,
                });
            }
        }
        let hash = parse_hash(target)
            .with_context(|| format!("cannot checkout '{target}': no such branch or commit"))?;
        self.read_commit(hash)?;
        atomic_write(&self.root.join(HEAD_FILE), hash.to_string().as_bytes())?;
        self.current_branch = None;
        self.head = Some(hash);
        Ok(Checkout {
            branch: None,
            commit: hash,
        })
    }

    /// Walks the first-parent commit history from HEAD, newest first.
    pub fn log(&self) -> Result<Vec<LogEntry>> {
        let mut entries = Vec::new();
        let mut current = self.head;
        while let Some(hash) = current {
            let commit = self.read_commit(hash)?;
            entries.push(LogEntry {
                hash,
                message: commit.message,
            });
            current = commit.parent;
        }
        Ok(entries)
    }

    /// Returns the current HEAD commit.
    pub fn head(&self) -> Option<Hash> {
        self.head
    }

    /// Returns the repository root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the backing object store.
    pub fn store(&self) -> &S {
        &self.store
    }

    fn write_commit(&self, tree: Hash, message: &str) -> Result<Hash> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before the unix epoch")?
            .as_secs();
        let commit = Commit {
            tree,
            parent: self.head,
            timestamp,
            message: message.to_string(),
        };
        self.write_object(&Object::Commit(commit))
    }

    fn finish_object_batch(&self, result: Result<Hash>) -> Result<Hash> {
        let hash = match result {
            Ok(hash) => hash,
            Err(error) => {
                self.store
                    .rollback_batch()
                    .context("failed to roll back object batch")?;
                return Err(error);
            }
        };
        if let Err(error) = self.store.commit_batch() {
            let _ = self.store.rollback_batch();
            return Err(error).context("failed to commit object batch");
        }
        Ok(hash)
    }

    fn publish_commit(&mut self, hash: Hash) -> Result<()> {
        match &self.current_branch {
            Some(name) => atomic_write(&self.ref_path(name)?, hash.to_string().as_bytes())?,
            None => atomic_write(&self.root.join(HEAD_FILE), hash.to_string().as_bytes())?,
        }
        self.head = Some(hash);
        Ok(())
    }

    fn refresh_head(&mut self) -> Result<()> {
        let (current_branch, head) = read_head(&self.root)?;
        self.current_branch = current_branch;
        self.head = head;
        Ok(())
    }

    fn empty_tree(&self) -> Result<Hash> {
        self.write_object(&Object::Tree(TreeNode::Branch(BTreeMap::new())))
    }

    fn write_object(&self, object: &Object) -> Result<Hash> {
        let bytes = object.to_bytes();
        let expected = object.hash();
        let actual = self.store.write(&bytes)?;
        if actual != expected {
            bail!("object store returned {actual} while writing {expected}");
        }
        Ok(actual)
    }

    fn read_object(&self, hash: Hash) -> Result<Object> {
        let bytes = self.store.read(hash)?;
        hash.verify(&bytes)?;
        Object::from_bytes(&bytes).with_context(|| format!("corrupt object {hash}"))
    }

    fn read_commit(&self, hash: Hash) -> Result<Commit> {
        match self.read_object(hash)? {
            Object::Commit(commit) => Ok(commit),
            other => bail!("object {hash} is not a commit: {other:?}"),
        }
    }

    fn apply_tree_changes(
        &self,
        existing: Option<Hash>,
        depth: usize,
        changes: &BTreeMap<ChunkCoords, Option<Hash>>,
    ) -> Result<Option<Hash>> {
        if changes.is_empty() {
            return Ok(existing);
        }
        if depth == TREE_DEPTH {
            if changes.len() != 1 {
                bail!("multiple coordinates resolved to one radix leaf");
            }
            let (&coords, &blob) = changes.iter().next().unwrap();
            if let Some(hash) = existing {
                match self.read_object(hash)? {
                    Object::Tree(TreeNode::Leaf {
                        coords: old_coords, ..
                    }) if old_coords == coords => {}
                    other => bail!("invalid existing leaf {hash}: {other:?}"),
                }
            }
            return match blob {
                Some(blob) => {
                    Ok(Some(self.write_object(&Object::Tree(TreeNode::Leaf {
                        coords,
                        blob,
                    }))?))
                }
                None => Ok(None),
            };
        }

        let original = match existing {
            Some(hash) => match self.read_object(hash)? {
                Object::Tree(TreeNode::Branch(children)) => children,
                other => bail!("invalid tree branch {hash} at depth {depth}: {other:?}"),
            },
            None => BTreeMap::new(),
        };
        let mut updated = original.clone();
        let mut groups: BTreeMap<u8, BTreeMap<ChunkCoords, Option<Hash>>> = BTreeMap::new();
        for (&coords, &blob) in changes {
            groups
                .entry(coord_nibble(coords, depth))
                .or_default()
                .insert(coords, blob);
        }
        for (nibble, group) in groups {
            let child =
                self.apply_tree_changes(updated.get(&nibble).copied(), depth + 1, &group)?;
            match child {
                Some(hash) => {
                    updated.insert(nibble, hash);
                }
                None => {
                    updated.remove(&nibble);
                }
            }
        }
        if updated == original {
            return Ok(existing);
        }
        if updated.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            self.write_object(&Object::Tree(TreeNode::Branch(updated)))?,
        ))
    }

    fn tree_entries(&self, root: Hash) -> Result<BTreeMap<ChunkCoords, Hash>> {
        let mut result = BTreeMap::new();
        let mut stack = vec![(root, 0usize, 0u64)];
        let mut visited = HashSet::new();
        while let Some((hash, depth, prefix)) = stack.pop() {
            if !visited.insert(hash) {
                bail!("tree node {hash} is referenced more than once");
            }
            match self.read_object(hash)? {
                Object::Tree(TreeNode::Branch(children)) if depth < TREE_DEPTH => {
                    for (&nibble, &child) in &children {
                        if nibble >= 16 {
                            bail!("tree node {hash} has invalid child nibble {nibble}");
                        }
                        let child_prefix = prefix | ((nibble as u64) << ((15 - depth) * 4));
                        stack.push((child, depth + 1, child_prefix));
                    }
                }
                Object::Tree(TreeNode::Leaf { coords, blob }) if depth == TREE_DEPTH => {
                    if coord_key(coords) != prefix {
                        bail!("tree leaf {hash} is stored under the wrong radix path");
                    }
                    if result.insert(coords, blob).is_some() {
                        bail!("tree contains duplicate coordinate {coords:?}");
                    }
                }
                other => bail!("invalid tree object {hash} at depth {depth}: {other:?}"),
            }
        }
        Ok(result)
    }

    fn ref_path(&self, name: &str) -> Result<PathBuf> {
        validate_branch_name(name)?;
        let refs = self.root.join(REFS_HEADS_DIR);
        let path = refs.join(name);
        if path.parent() != Some(refs.as_path()) {
            bail!("branch path escapes refs directory: {name}");
        }
        Ok(path)
    }

    fn lock(&self) -> Result<RepositoryLock> {
        RepositoryLock::acquire(self.root.join(LOCK_FILE))
    }
}

fn coord_nibble(coords: ChunkCoords, depth: usize) -> u8 {
    let mut bytes = [0u8; 8];
    bytes[..4].copy_from_slice(&coords.0.to_be_bytes());
    bytes[4..].copy_from_slice(&coords.1.to_be_bytes());
    let byte = bytes[depth / 2];
    if depth % 2 == 0 {
        byte >> 4
    } else {
        byte & 0x0f
    }
}

fn coord_key(coords: ChunkCoords) -> u64 {
    ((coords.0 as u32 as u64) << 32) | coords.1 as u32 as u64
}

fn initialize_repository(path: &Path) -> Result<()> {
    let chunklog_dir = path.join(CHUNKLOG_DIR);
    if chunklog_dir.exists() {
        bail!("repository already exists at {}", path.display());
    }
    fs::create_dir_all(chunklog_dir.join("objects"))?;
    fs::create_dir_all(chunklog_dir.join("refs/heads"))?;
    fs::create_dir_all(path.join(STAGING_DIR))?;
    atomic_write(&path.join(FORMAT_FILE), REPOSITORY_FORMAT.as_bytes())?;
    atomic_write(
        &path.join(HEAD_FILE),
        format!("ref: refs/heads/{DEFAULT_BRANCH}").as_bytes(),
    )?;
    Ok(())
}

fn validate_repository(path: &Path) -> Result<()> {
    let chunklog_dir = path.join(CHUNKLOG_DIR);
    if !chunklog_dir.is_dir() {
        bail!("not a chunklog repository at {}", path.display());
    }
    let format = fs::read_to_string(path.join(FORMAT_FILE)).context(
        "repository has no supported FORMAT marker; pre-v2 repositories require migration",
    )?;
    if format.trim() != REPOSITORY_FORMAT.trim() {
        bail!("unsupported repository format: {}", format.trim());
    }
    Ok(())
}

fn read_head(path: &Path) -> Result<(Option<String>, Option<Hash>)> {
    let contents = fs::read_to_string(path.join(HEAD_FILE)).context("failed to read HEAD")?;
    let trimmed = contents.trim();
    if let Some(branch) = trimmed.strip_prefix("ref: refs/heads/") {
        validate_branch_name(branch).with_context(|| format!("corrupt HEAD: {trimmed}"))?;
        let ref_path = path.join(REFS_HEADS_DIR).join(branch);
        return Ok((Some(branch.to_string()), read_branch_ref_path(&ref_path)?));
    }
    if trimmed.is_empty() {
        bail!("corrupt HEAD: empty");
    }
    let hash = parse_hash(trimmed).with_context(|| format!("corrupt HEAD: {trimmed}"))?;
    Ok((None, Some(hash)))
}

fn read_branch_ref_path(path: &Path) -> Result<Option<Hash>> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let trimmed = contents.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(parse_hash(trimmed).with_context(|| {
                    format!("invalid branch reference in {}", path.display())
                })?))
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn validate_branch_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("branch name cannot be empty");
    }
    if name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains(char::is_whitespace)
        || name.starts_with('.')
        || name.ends_with('.')
        || name.ends_with(' ')
        || name.contains("..")
        || Path::new(name).is_absolute()
    {
        bail!("invalid branch name: {name}");
    }
    Ok(())
}

fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)?;
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| -> std::io::Result<()> {
            file.write_all(data)?;
            Ok(())
        })
        .map_err(|error| anyhow!(error))
        .with_context(|| format!("failed to atomically write {}", path.display()))
}

struct RepositoryLock {
    path: PathBuf,
}

impl RepositoryLock {
    fn acquire(path: PathBuf) -> Result<Self> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| {
                format!(
                    "repository is locked at {}; remove a stale lock only after confirming no writer is active",
                    path.display()
                )
            })?;
        writeln!(file, "pid={}", std::process::id())?;
        file.sync_all()?;
        Ok(Self { path })
    }
}

impl Drop for RepositoryLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
