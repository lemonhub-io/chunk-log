use std::collections::BTreeMap;

use chunklog::{Commit, Hash, Object};

#[test]
fn object_roundtrip() {
    let objects = vec![
        Object::Blob(vec![1, 2, 3]),
        Object::Tree(BTreeMap::from([((0, 0), Hash([7; 8]))])),
        Object::Commit(Commit {
            tree: Hash([1; 8]),
            parent: Some(Hash([2; 8])),
            timestamp: 42,
            message: "save".to_string(),
        }),
    ];
    for object in objects {
        let bytes = object.to_bytes();
        assert_eq!(Object::from_bytes(&bytes).unwrap(), object);
    }
}

#[test]
fn hash_is_deterministic_and_content_addressed() {
    let a = Object::Blob(vec![1, 2, 3]);
    let b = Object::Blob(vec![1, 2, 3]);
    let c = Object::Blob(vec![1, 2, 4]);
    assert_eq!(a.hash(), b.hash());
    assert_ne!(a.hash(), c.hash());
}

#[test]
fn hash_parses_and_formats_as_hex() {
    let hash = Hash([0xab, 0xcd, 0xef, 0, 1, 2, 3, 4]);
    let text = hash.to_string();
    assert_eq!(text, "abcdef0001020304");
    assert_eq!(chunklog::parse_hash(&text).unwrap(), hash);
}
