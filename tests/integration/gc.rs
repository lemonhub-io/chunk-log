use std::collections::HashMap;

use chunklog::{ObjectStore, Repository};
use tempfile::tempdir;

fn world(chunk: (i32, i32), data: u8) -> HashMap<(i32, i32), Vec<u8>> {
    let mut world = HashMap::new();
    world.insert(chunk, vec![data]);
    world
}

#[test]
fn gc_removes_unreachable_objects() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
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

#[test]
fn gc_on_fresh_repo_removes_nothing() {
    let dir = tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    let stats = repo.collect_garbage().unwrap();
    assert_eq!(stats.removed, 0);
    assert_eq!(stats.retained, 0);
}

#[test]
fn gc_keeps_shared_objects() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let c1 = repo.commit(&world((0, 0), 1), "one").unwrap();
    let c2 = repo.commit(&world((0, 0), 1), "two").unwrap();

    let stats = repo.collect_garbage().unwrap();
    assert_eq!(stats.removed, 0);
    assert_eq!(stats.retained, 4);
    assert_eq!(repo.load(c1).unwrap(), repo.load(c2).unwrap());
}

#[test]
fn gc_respects_all_branch_refs() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let main_commit = repo.commit(&world((0, 0), 1), "main").unwrap();
    repo.create_branch("backup").unwrap();

    let mut extra = world((5, 5), 9);
    extra.insert((6, 6), vec![8]);
    repo.checkout("main").unwrap();
    let extra_commit = repo.commit(&extra, "extra").unwrap();

    // HEAD is on backup (at main_commit); the extra commit is only
    // reachable through the main branch, not through HEAD.
    repo.checkout("backup").unwrap();
    assert_ne!(repo.head(), Some(extra_commit));

    let stats = repo.collect_garbage().unwrap();
    assert_eq!(stats.removed, 0);
    assert_eq!(repo.load(main_commit).unwrap(), world((0, 0), 1));
    assert_eq!(repo.load(extra_commit).unwrap(), extra);
}
