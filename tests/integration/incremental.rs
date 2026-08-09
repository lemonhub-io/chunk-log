use std::collections::HashMap;

use chunklog::{ChangeSet, ObjectStore, Repository};
use tempfile::tempdir;

#[test]
fn change_set_preserves_unmentioned_chunks() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let world = HashMap::from([((0, 0), vec![1]), ((1, 1), vec![2]), ((-5, 9), vec![3])]);
    repo.commit_snapshot(&world, "base").unwrap();

    let mut changes = ChangeSet::new();
    changes.upsert((1, 1), vec![8]);
    changes.remove((-5, 9));
    changes.upsert((7, 7), vec![4]);
    let commit = repo.commit_changes(&changes, "patch").unwrap();

    let expected = HashMap::from([((0, 0), vec![1]), ((1, 1), vec![8]), ((7, 7), vec![4])]);
    assert_eq!(repo.load(commit).unwrap(), expected);
}

#[test]
fn one_change_publishes_only_one_radix_path() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let world: HashMap<_, _> = (0i32..1_000)
        .map(|i| ((i, i * 3), i.to_be_bytes().to_vec()))
        .collect();
    repo.commit_snapshot(&world, "base").unwrap();
    let before = repo.store().list().unwrap().len();

    let mut changes = ChangeSet::new();
    changes.upsert((500, 1_500), b"changed".to_vec());
    repo.commit_changes(&changes, "one change").unwrap();
    let published = repo.store().list().unwrap().len() - before;

    // blob + leaf + at most sixteen branch nodes + commit
    assert!(published <= 19, "published {published} objects");
    assert!(published >= 3);
}

#[test]
fn empty_change_set_reuses_tree() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.commit_snapshot(&HashMap::from([((0, 0), vec![1])]), "base")
        .unwrap();
    let before = repo.store().list().unwrap().len();
    repo.commit_changes(&ChangeSet::new(), "metadata only")
        .unwrap();
    assert_eq!(repo.store().list().unwrap().len() - before, 1);
}

#[test]
fn deterministic_change_sequence_matches_reference_world() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    repo.commit_snapshot(&HashMap::new(), "empty").unwrap();
    let mut expected = HashMap::new();
    let mut state = 0x9e37_79b9_u32;

    for round in 0..100u32 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let coords = ((state % 31) as i32 - 15, ((state >> 8) % 31) as i32 - 15);
        let mut changes = ChangeSet::new();
        if state % 5 == 0 {
            changes.remove(coords);
            expected.remove(&coords);
        } else {
            let payload = [round.to_be_bytes(), state.to_be_bytes()].concat();
            changes.upsert(coords, payload.clone());
            expected.insert(coords, payload);
        }
        let commit = repo
            .commit_changes(&changes, "randomized deterministic step")
            .unwrap();
        assert_eq!(repo.load(commit).unwrap(), expected);
    }
}
