use chunklog::{FilesystemStore, Object, ObjectStore};
use tempfile::tempdir;

#[test]
fn write_read_roundtrip() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path());
    let object = Object::Blob(vec![1, 2, 3]);
    let hash = store.write(&object).unwrap();
    assert_eq!(store.read(hash).unwrap(), object);
}

#[test]
fn deduplicates_identical_objects() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path());
    let object = Object::Blob(vec![5, 6]);
    let h1 = store.write(&object).unwrap();
    let h2 = store.write(&object).unwrap();
    assert_eq!(h1, h2);
    assert_eq!(dir.path().read_dir().unwrap().count(), 1);
}

#[test]
fn hash_is_deterministic() {
    let a = Object::Blob(vec![1, 2, 3]);
    let b = Object::Blob(vec![1, 2, 3]);
    assert_eq!(a.hash(), b.hash());
    assert_ne!(a.hash(), Object::Blob(vec![1, 2, 4]).hash());
}
