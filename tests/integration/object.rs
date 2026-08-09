use std::collections::BTreeMap;

use chunklog::{Commit, Hash, Object, TreeNode};

#[test]
fn object_roundtrip() {
    let objects = vec![
        Object::Blob(vec![0, 1, 2, 255]),
        Object::Tree(TreeNode::Branch(BTreeMap::from([(7, Hash([7; 32]))]))),
        Object::Tree(TreeNode::Leaf {
            coords: (0, 0),
            blob: Hash([8; 32]),
        }),
        Object::Commit(Commit {
            tree: Hash([1; 32]),
            parent: Some(Hash([2; 32])),
            timestamp: 42,
            message: "save".to_string(),
        }),
    ];
    for object in objects {
        let bytes = object.to_bytes();
        assert_eq!(Object::from_bytes(&bytes).unwrap(), object);

        for end in 0..bytes.len() {
            assert!(Object::from_bytes(&bytes[..end]).is_err());
        }
    }

    // A deterministic byte corpus exercises parser lengths, tags and trailing-data
    // rejection without depending on a platform-specific fuzzing harness.
    let mut state = 0x6a09_e667_f3bc_c909_u64;
    for len in 0..256 {
        let mut candidate = Vec::with_capacity(len);
        for _ in 0..len {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            candidate.push((state >> 56) as u8);
        }
        if let Ok(parsed) = Object::from_bytes(&candidate) {
            assert_eq!(parsed.to_bytes(), candidate);
        }
    }
}

#[test]
fn hash_is_deterministic_and_content_addressed() {
    let a = Object::Tree(TreeNode::Branch(BTreeMap::from([(0, Hash([1; 32]))])));
    let b = Object::Tree(TreeNode::Branch(BTreeMap::from([(0, Hash([1; 32]))])));
    let c = Object::Tree(TreeNode::Branch(BTreeMap::from([(0, Hash([2; 32]))])));
    assert_eq!(a.hash(), b.hash());
    assert_ne!(a.hash(), c.hash());

    let payload = vec![1, 0, 0, 0, 0];
    let blob = Object::Blob(payload.clone());
    let branch = Object::Tree(TreeNode::Branch(BTreeMap::new()));

    assert_ne!(blob.to_bytes()[5], branch.to_bytes()[5]);
    assert_ne!(blob.hash(), branch.hash());
}

#[test]
fn hash_parses_and_formats_as_hex() {
    let hash = Hash([0xab; 32]);
    let text = hash.to_string();
    assert_eq!(text.len(), 64);
    assert_eq!(text, "ab".repeat(32));
    assert_eq!(chunklog::parse_hash(&text).unwrap(), hash);
}
