use std::collections::HashMap;
use std::fs;

use chunklog::{FilesystemStore, ObjectStore, Repository};
use rusqlite::{params, Connection};
use tempfile::tempdir;

#[test]
fn filesystem_store_rejects_tampered_content() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path());
    let original = b"addressed bytes";
    let hash = store.write(original).unwrap();
    fs::write(dir.path().join(hash.to_string()), b"tampered bytes").unwrap();
    let error = store.read(hash).unwrap_err().to_string();
    assert!(error.contains("integrity") || error.contains("corrupt"));
    assert!(store.write(original).is_err());
}

#[test]
fn repository_rejects_a_tampered_blob() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let commit = repo
        .commit_snapshot(&HashMap::from([((0, 0), vec![1, 2, 3])]), "base")
        .unwrap();
    let blob = repo.chunk_hashes(commit).unwrap()[0].1;
    let connection = Connection::open(dir.path().join(".chunklog/objects.sqlite3")).unwrap();
    connection
        .execute(
            "UPDATE objects SET data = ?1 WHERE hash = ?2",
            params![b"silent corruption".as_slice(), &blob.0[..]],
        )
        .unwrap();
    assert!(repo.load(commit).is_err());
    assert!(repo.collect_garbage().is_err());
}

#[test]
fn branch_paths_cannot_escape_refs_directory() {
    let dir = tempdir().unwrap();
    let sentinel = dir.path().join("sentinel");
    fs::write(&sentinel, b"keep").unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.commit_snapshot(&HashMap::new(), "base").unwrap();

    for invalid in ["..", "../sentinel", "a/b", "a\\b", ".hidden"] {
        assert!(
            repo.create_branch(invalid).is_err(),
            "create accepted {invalid}"
        );
        assert!(
            repo.delete_branch(invalid).is_err(),
            "delete accepted {invalid}"
        );
        assert!(
            repo.checkout(invalid).is_err(),
            "checkout accepted {invalid}"
        );
        assert!(repo.resolve(invalid).is_err(), "resolve accepted {invalid}");
    }
    assert_eq!(fs::read(sentinel).unwrap(), b"keep");
}

#[test]
fn unsupported_or_missing_repository_format_is_rejected() {
    let dir = tempdir().unwrap();
    Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join(".chunklog/FORMAT"), "1\n").unwrap();
    assert!(Repository::open(dir.path()).is_err());
    fs::write(dir.path().join(".chunklog/FORMAT"), "999\n").unwrap();
    assert!(Repository::open(dir.path()).is_err());
    fs::remove_file(dir.path().join(".chunklog/FORMAT")).unwrap();
    assert!(Repository::open(dir.path()).is_err());
}

#[test]
fn repository_lock_prevents_overlapping_writers() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let original = repo
        .commit_snapshot(&HashMap::from([((0, 0), vec![1])]), "base")
        .unwrap();
    fs::write(dir.path().join(".chunklog/LOCK"), "pid=test\n").unwrap();
    let error = repo
        .commit_snapshot(&HashMap::from([((0, 0), vec![2])]), "blocked")
        .unwrap_err()
        .to_string();
    assert!(error.contains("locked"));
    assert_eq!(repo.head(), Some(original));
}
