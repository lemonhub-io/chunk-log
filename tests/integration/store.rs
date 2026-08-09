use chunklog::{FilesystemStore, ObjectStore};
use tempfile::tempdir;

#[test]
fn write_read_roundtrip() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path());
    let data = vec![1, 2, 3];
    let hash = store.write(&data).unwrap();
    assert_eq!(store.read(hash).unwrap(), data);
}

#[test]
fn deduplicates_identical_data() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path());
    let h1 = store.write(b"chunk").unwrap();
    let h2 = store.write(b"chunk").unwrap();
    assert_eq!(h1, h2);
    assert_eq!(dir.path().read_dir().unwrap().count(), 1);
}

#[test]
fn hash_is_content_addressed() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path());
    let hash = store.write(b"hello world").unwrap();
    assert_eq!(hash.to_string().len(), 64);
    assert_ne!(hash, store.write(b"hello worlD").unwrap());
}

#[test]
fn list_and_delete() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path());
    let h1 = store.write(b"one").unwrap();
    let h2 = store.write(b"two").unwrap();
    let mut expected = vec![h1, h2];
    expected.sort();
    let mut listed = store.list().unwrap();
    listed.sort();
    assert_eq!(listed, expected);

    store.delete(h1).unwrap();
    assert_eq!(store.list().unwrap(), vec![h2]);
    store.delete(h1).unwrap();
}
