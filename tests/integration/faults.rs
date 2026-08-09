use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use std::sync::RwLock;

use anyhow::{anyhow, bail, Result};
use chunklog::{Hash, Object, ObjectStore, Repository};
use tempfile::tempdir;

struct FaultingStore {
    objects: RwLock<HashMap<Hash, Vec<u8>>>,
    fail_delete_at: AtomicIsize,
    delete_calls: AtomicUsize,
}

impl FaultingStore {
    fn new() -> Self {
        Self {
            objects: RwLock::new(HashMap::new()),
            fail_delete_at: AtomicIsize::new(-1),
            delete_calls: AtomicUsize::new(0),
        }
    }

    fn fail_delete_at(&self, call: isize) {
        self.delete_calls.store(0, Ordering::SeqCst);
        self.fail_delete_at.store(call, Ordering::SeqCst);
    }

    fn corrupt(&self, hash: Hash) {
        self.objects
            .write()
            .unwrap()
            .insert(hash, b"corrupt".to_vec());
    }
}

impl ObjectStore for FaultingStore {
    fn read(&self, hash: Hash) -> Result<Vec<u8>> {
        self.objects
            .read()
            .unwrap()
            .get(&hash)
            .cloned()
            .ok_or_else(|| anyhow!("missing {hash}"))
    }

    fn write(&self, bytes: &[u8]) -> Result<Hash> {
        let hash = Hash::digest(bytes);
        let mut objects = self.objects.write().unwrap();
        if let Some(existing) = objects.get(&hash) {
            hash.verify(existing)?;
        } else {
            objects.insert(hash, bytes.to_vec());
        }
        Ok(hash)
    }

    fn list(&self) -> Result<Vec<Hash>> {
        Ok(self.objects.read().unwrap().keys().copied().collect())
    }

    fn delete(&self, hash: Hash) -> Result<()> {
        let call = self.delete_calls.fetch_add(1, Ordering::SeqCst) as isize;
        if call == self.fail_delete_at.load(Ordering::SeqCst) {
            bail!("injected delete failure at call {call}");
        }
        self.objects.write().unwrap().remove(&hash);
        Ok(())
    }
}

#[test]
fn marking_failure_happens_before_any_deletion() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init_with(FaultingStore::new(), dir.path()).unwrap();
    let commit = repo
        .commit_snapshot(&HashMap::from([((0, 0), vec![1])]), "main")
        .unwrap();
    let blob = repo.chunk_hashes(commit).unwrap()[0].1;
    let orphan = repo
        .store()
        .write(&Object::Blob(vec![99]).to_bytes())
        .unwrap();
    repo.store().corrupt(blob);

    assert!(repo.collect_garbage().is_err());
    assert_eq!(repo.store().delete_calls.load(Ordering::SeqCst), 0);
    assert!(repo.store().list().unwrap().contains(&orphan));
}

#[test]
fn interrupted_sweep_is_safe_to_retry() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init_with(FaultingStore::new(), dir.path()).unwrap();
    let main = repo
        .commit_snapshot(&HashMap::from([((0, 0), vec![1])]), "main")
        .unwrap();
    repo.create_branch("discard").unwrap();
    repo.checkout("discard").unwrap();
    repo.commit_snapshot(&HashMap::from([((9, 9), vec![2])]), "discard")
        .unwrap();
    repo.checkout("main").unwrap();
    repo.delete_branch("discard").unwrap();

    repo.store().fail_delete_at(1);
    assert!(repo.collect_garbage().is_err());
    assert_eq!(repo.load(main).unwrap().get(&(0, 0)).unwrap(), &[1]);

    repo.store().fail_delete_at(-1);
    let stats = repo.collect_garbage().unwrap();
    assert!(stats.removed > 0);
    assert_eq!(repo.load(main).unwrap().get(&(0, 0)).unwrap(), &[1]);
}
