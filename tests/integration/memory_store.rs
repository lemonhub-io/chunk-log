use std::collections::HashMap;
use std::sync::RwLock;

use anyhow::{anyhow, Result};
use chunklog::{Hash, ObjectStore, Repository};
use tempfile::tempdir;
use xxhash_rust::xxh3::xxh3_64;

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
        let hash = Hash(xxh3_64(data).to_le_bytes());
        self.0
            .write()
            .unwrap()
            .entry(hash)
            .or_insert_with(|| data.to_vec());
        Ok(hash)
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
