use std::collections::HashMap;

use chunklog::Repository;
use tempfile::tempdir;

fn world() -> HashMap<(i32, i32), Vec<u8>> {
    let mut world = HashMap::new();
    world.insert((0, 0), vec![1, 2, 3]);
    world
}

#[test]
fn init_creates_unborn_main_branch() {
    let dir = tempdir().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    assert_eq!(repo.current_branch().unwrap(), "main");
    assert_eq!(repo.head(), None);
    assert!(repo.branches().unwrap().is_empty());
    assert!(repo.log().unwrap().is_empty());
}

#[test]
fn commit_updates_current_branch_ref() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let hash = repo.commit(&world(), "first").unwrap();

    let branches = repo.branches().unwrap();
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].name, "main");
    assert_eq!(branches[0].commit, Some(hash));
}

#[test]
fn create_and_switch_branches() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let first = repo.commit(&world(), "first").unwrap();

    repo.create_branch("experiment").unwrap();
    let checkout = repo.checkout("experiment").unwrap();
    assert_eq!(checkout.branch.as_deref(), Some("experiment"));
    assert_eq!(checkout.commit, first);
    assert_eq!(repo.current_branch().unwrap(), "experiment");

    let mut extended = world();
    extended.insert((1, 1), vec![9, 9]);
    let experiment_commit = repo.commit(&extended, "on experiment").unwrap();

    let branches = repo.branches().unwrap();
    let main = branches.iter().find(|b| b.name == "main").unwrap();
    let experiment = branches.iter().find(|b| b.name == "experiment").unwrap();
    assert_eq!(main.commit, Some(first));
    assert_eq!(experiment.commit, Some(experiment_commit));

    let checkout = repo.checkout("main").unwrap();
    assert_eq!(checkout.branch.as_deref(), Some("main"));
    assert_eq!(repo.head(), Some(first));
    assert_eq!(repo.load(first).unwrap(), world());
}

#[test]
fn checkout_detached_commit() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let first = repo.commit(&world(), "first").unwrap();
    let mut extended = world();
    extended.insert((1, 1), vec![2]);
    let second = repo.commit(&extended, "second").unwrap();

    let checkout = repo.checkout(&first.to_string()).unwrap();
    assert!(checkout.branch.is_none());
    assert_eq!(repo.current_branch(), None);
    assert_eq!(repo.head(), Some(first));

    let mut detached_work = world();
    detached_work.insert((5, 5), vec![7]);
    let third = repo.commit(&detached_work, "detached work").unwrap();
    assert_eq!(repo.current_branch(), None);
    assert_eq!(repo.head(), Some(third));

    repo.checkout("main").unwrap();
    assert_eq!(repo.head(), Some(second));
}

#[test]
fn load_returns_world_of_commit() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut extended = world();
    extended.insert((-3, 4), vec![42, 42]);
    let hash = repo.commit(&extended, "save").unwrap();

    assert_eq!(repo.load(hash).unwrap(), extended);

    let hashes = repo.chunk_hashes(hash).unwrap();
    assert_eq!(hashes.len(), 2);
    assert!(hashes.iter().any(|(coords, _)| *coords == (0, 0)));
    assert!(hashes.iter().any(|(coords, _)| *coords == (-3, 4)));
}

#[test]
fn delete_branch() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.commit(&world(), "first").unwrap();

    repo.create_branch("feature").unwrap();
    repo.delete_branch("feature").unwrap();
    assert!(repo.branches().unwrap().iter().all(|b| b.name != "feature"));

    let err = repo.delete_branch("main").unwrap_err();
    assert!(err.to_string().contains("current branch"));
}

#[test]
fn branches_persist_across_open() {
    let dir = tempdir().unwrap();
    {
        let mut repo = Repository::init(dir.path()).unwrap();
        repo.commit(&world(), "first").unwrap();
        repo.create_branch("feature").unwrap();
        repo.checkout("feature").unwrap();
        repo.commit(&world(), "second").unwrap();
    }
    let repo = Repository::open(dir.path()).unwrap();
    assert_eq!(repo.current_branch().unwrap(), "feature");
    assert_eq!(repo.branches().unwrap().len(), 2);
    assert_eq!(repo.log().unwrap().len(), 2);
}

#[test]
fn checkout_unknown_target_fails() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.commit(&world(), "first").unwrap();

    assert!(repo.checkout("nope").is_err());
    assert!(repo.checkout(&"f".repeat(64)).is_err());
}

#[test]
fn duplicate_branch_name_rejected() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.commit(&world(), "first").unwrap();

    repo.create_branch("feature").unwrap();
    assert!(repo.create_branch("feature").is_err());
    assert!(repo.create_branch("bad name").is_err());
    assert!(repo.create_branch("../evil").is_err());
}
