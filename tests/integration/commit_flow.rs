use std::collections::HashMap;

use chunklog::{Object, ObjectStore, Repository};
use tempfile::tempdir;

fn chunks() -> HashMap<(i32, i32), Vec<u8>> {
    let mut chunks = HashMap::new();
    chunks.insert((0, 0), vec![1, 2, 3]);
    chunks.insert((-4, 7), vec![9, 9, 9]);
    chunks
}

#[test]
fn init_commit_and_log() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();

    let hash = repo.commit(&chunks(), "first save").unwrap();
    assert_eq!(repo.head(), Some(hash));

    let log = repo.log().unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].0, hash);
    assert_eq!(log[0].1, "first save");
}

#[test]
fn commit_chains_history() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();

    let mut world = chunks();
    let first = repo.commit(&world, "first").unwrap();
    world.insert((1, 1), vec![2]);
    let second = repo.commit(&world, "second").unwrap();

    let log = repo.log().unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].0, second);
    assert_eq!(log[0].1, "second");
    assert_eq!(log[1].0, first);
    assert_eq!(log[1].1, "first");
}

#[test]
fn repository_persists_across_open() {
    let dir = tempdir().unwrap();
    {
        let mut repo = Repository::init(dir.path()).unwrap();
        repo.commit(&chunks(), "persisted").unwrap();
    }
    let repo = Repository::open(dir.path()).unwrap();
    assert!(repo.head().is_some());
    assert_eq!(repo.log().unwrap().len(), 1);
}

#[test]
fn identical_worlds_share_tree_and_blobs() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();

    let first = repo.commit(&chunks(), "first").unwrap();
    let second = repo.commit(&chunks(), "second").unwrap();

    assert_ne!(first, second);

    let store = repo.store();
    let Object::Commit(c1) = store.read(first).unwrap() else {
        panic!("{first} is not a commit");
    };
    let Object::Commit(c2) = store.read(second).unwrap() else {
        panic!("{second} is not a commit");
    };
    assert_eq!(c1.tree, c2.tree);
    assert_eq!(c2.parent, Some(first));

    let Object::Tree(t1) = store.read(c1.tree).unwrap() else {
        panic!("{} is not a tree", c1.tree);
    };
    let Object::Tree(t2) = store.read(c2.tree).unwrap() else {
        panic!("{} is not a tree", c2.tree);
    };
    assert_eq!(t1, t2);
}

#[test]
fn log_after_open_walks_parent_chain() {
    let dir = tempdir().unwrap();
    {
        let mut repo = Repository::init(dir.path()).unwrap();
        repo.commit(&chunks(), "a").unwrap();
        repo.commit(&chunks(), "b").unwrap();
    }
    let repo = Repository::open(dir.path()).unwrap();
    let log = repo.log().unwrap();
    assert_eq!(log.iter().map(|(_, m)| m.as_str()).collect::<Vec<_>>(), ["b", "a"]);
}
