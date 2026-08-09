use std::collections::HashMap;

use chunklog::{Hash, Repository};
use tempfile::tempdir;

fn world1() -> HashMap<(i32, i32), Vec<u8>> {
    let mut world = HashMap::new();
    world.insert((0, 0), vec![1]);
    world.insert((1, 1), vec![2]);
    world
}

fn world2() -> HashMap<(i32, i32), Vec<u8>> {
    let mut world = HashMap::new();
    world.insert((0, 0), vec![1, 1]);
    world.insert((2, 2), vec![3]);
    world
}

#[test]
fn diff_detects_added_modified_removed() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let c1 = repo.commit(&world1(), "one").unwrap();
    let c2 = repo.commit(&world2(), "two").unwrap();

    let diff = repo.diff(Some(c1), c2).unwrap();
    assert_eq!(diff.added.len(), 1);
    assert_eq!(diff.added[0].0, (2, 2));
    assert_eq!(diff.modified.len(), 1);
    assert_eq!(diff.modified[0].0, (0, 0));
    assert_ne!(diff.modified[0].1 .0, diff.modified[0].1 .1);
    assert_eq!(diff.removed.len(), 1);
    assert_eq!(diff.removed[0].0, (1, 1));
}

#[test]
fn diff_against_empty_lists_everything() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let c1 = repo.commit(&world1(), "one").unwrap();

    let diff = repo.diff(None, c1).unwrap();
    assert_eq!(diff.added.len(), 2);
    assert!(diff.modified.is_empty());
    assert!(diff.removed.is_empty());
    assert_eq!(diff.len(), 2);
}

#[test]
fn diff_same_commit_is_empty() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let c1 = repo.commit(&world1(), "one").unwrap();
    assert!(repo.diff(Some(c1), c1).unwrap().is_empty());
}

#[test]
fn diff_requires_commits() {
    let dir = tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    assert!(repo.diff(None, Hash([0xff; 32])).is_err());
}

#[test]
fn resolve_branch_and_hash() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let hash = repo.commit(&world1(), "one").unwrap();
    repo.create_branch("feature").unwrap();

    assert_eq!(repo.resolve("main").unwrap(), hash);
    assert_eq!(repo.resolve(&hash.to_string()).unwrap(), hash);
    assert!(repo.resolve("nope").is_err());
}
