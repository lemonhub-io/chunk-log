use std::collections::HashMap;
use std::sync::RwLock;

use anyhow::{anyhow, Result};
use chunklog::{Hash, ObjectStore, Repository};
use tempfile::tempdir;

/// An in-memory [`ObjectStore`] used to prove that `Repository` works
/// with any backend, not just the filesystem.
struct MemoryStore(RwLock<HashMap<Hash, Vec<u8>>>);

impl MemoryStore {
    fn new() -> Self {
        Self(RwLock::new(HashMap::new()))
    }
}

impl ObjectStore for MemoryStore {
    fn read(&self, hash: Hash) -> Result<Vec<u8>> {
        self.0
            .read()
            .unwrap()
            .get(&hash)
            .cloned()
            .ok_or_else(|| anyhow!("object {hash} not found"))
    }

    fn write(&self, data: &[u8]) -> Result<Hash> {
        let hash = Hash(blake3::hash(data).into());
        self.0
            .write()
            .unwrap()
            .entry(hash)
            .or_insert_with(|| data.to_vec());
        Ok(hash)
    }

    fn list(&self) -> Result<Vec<Hash>> {
        Ok(self.0.read().unwrap().keys().copied().collect())
    }

    fn delete(&self, hash: Hash) -> Result<()> {
        self.0.write().unwrap().remove(&hash);
        Ok(())
    }
}

#[test]
fn repository_works_with_any_object_store() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init_with(MemoryStore::new(), dir.path()).unwrap();

    let mut world = HashMap::new();
    world.insert((0, 0), vec![1, 2, 3]);
    world.insert((-4, 7), vec![4, 5, 6]);
    let hash = repo.commit(&world, "in memory").unwrap();

    assert_eq!(repo.head(), Some(hash));
    let log = repo.log().unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].hash, hash);
}

#[test]
fn garbage_collection_works_with_any_store() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init_with(MemoryStore::new(), dir.path()).unwrap();

    let main_commit = repo.commit(&world((0, 0), 1), "main").unwrap();
    repo.create_branch("feature").unwrap();
    repo.checkout("feature").unwrap();
    let feature_commit = repo.commit(&world((1, 1), 2), "feature").unwrap();
    repo.checkout("main").unwrap();
    repo.delete_branch("feature").unwrap();

    let stats = repo.collect_garbage().unwrap();
    assert_eq!(stats.removed, 3);
    assert_eq!(stats.retained, 3);
    assert_eq!(repo.load(main_commit).unwrap(), world((0, 0), 1));
    assert!(repo.store().read(feature_commit).is_err());
}

fn world(chunk: (i32, i32), data: u8) -> HashMap<(i32, i32), Vec<u8>> {
    let mut world = HashMap::new();
    world.insert(chunk, vec![data]);
    world
}
