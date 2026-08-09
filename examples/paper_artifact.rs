//! Reproducible structural-growth experiment used by the paper.
//!
//! Run with `cargo run --release --example paper_artifact`. The experiment
//! uses a verified in-memory object store so results measure the canonical
//! graph rather than filesystem allocation policy.

use std::collections::HashMap;
use std::sync::RwLock;

use anyhow::{anyhow, Result};
use chunklog::{ChangeSet, Hash, Object, ObjectStore, Repository, TreeNode};
use tempfile::tempdir;

const N: usize = 1_024;
const R: usize = 50;
const PAYLOAD_BYTES: usize = 256;

#[derive(Default)]
struct VerifiedMemoryStore(RwLock<HashMap<Hash, Vec<u8>>>);

impl ObjectStore for VerifiedMemoryStore {
    fn read(&self, hash: Hash) -> Result<Vec<u8>> {
        let bytes = self
            .0
            .read()
            .unwrap()
            .get(&hash)
            .cloned()
            .ok_or_else(|| anyhow!("object {hash} not found"))?;
        hash.verify(&bytes)?;
        Ok(bytes)
    }

    fn write(&self, bytes: &[u8]) -> Result<Hash> {
        let hash = Hash::digest(bytes);
        let mut objects = self.0.write().unwrap();
        if let Some(existing) = objects.get(&hash) {
            hash.verify(existing)?;
        } else {
            objects.insert(hash, bytes.to_vec());
        }
        Ok(hash)
    }

    fn list(&self) -> Result<Vec<Hash>> {
        Ok(self.0.read().unwrap().keys().copied().collect())
    }

    fn delete(&self, hash: Hash) -> Result<()> {
        self.0.write().unwrap().remove(&hash);
        Ok(())
    }
}

#[derive(Default)]
struct Counts {
    blobs: usize,
    branches: usize,
    leaves: usize,
    commits: usize,
    bytes: usize,
}

fn payload(version: usize, index: usize) -> Vec<u8> {
    let mut payload = vec![0u8; PAYLOAD_BYTES];
    payload[..8].copy_from_slice(&(version as u64).to_be_bytes());
    payload[8..16].copy_from_slice(&(index as u64).to_be_bytes());
    let mut state = (version as u64)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(index as u64);
    for byte in &mut payload[16..] {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        *byte = (state >> 56) as u8;
    }
    payload
}

fn coords(index: usize) -> (i32, i32) {
    (index as i32, (index as i32).wrapping_mul(3))
}

fn run(k: usize) -> Result<Counts> {
    let dir = tempdir()?;
    let mut repo = Repository::init_with(VerifiedMemoryStore::default(), dir.path())?;
    let world = (0..N)
        .map(|index| (coords(index), payload(0, index)))
        .collect();
    repo.commit_snapshot(&world, "initial")?;
    for round in 1..=R {
        let mut changes = ChangeSet::new();
        for offset in 0..k {
            // Rotate the edited window to exercise both shared and distinct
            // radix prefixes while keeping the coordinate set fixed.
            let index = (round * 131 + offset) % N;
            changes.upsert(coords(index), payload(round, index));
        }
        repo.commit_changes(&changes, "controlled edit")?;
    }

    let mut counts = Counts::default();
    for hash in repo.store().list()? {
        let bytes = repo.store().read(hash)?;
        counts.bytes += bytes.len();
        match Object::from_bytes(&bytes)? {
            Object::Blob(_) => counts.blobs += 1,
            Object::Tree(TreeNode::Branch(_)) => counts.branches += 1,
            Object::Tree(TreeNode::Leaf { .. }) => counts.leaves += 1,
            Object::Commit(_) => counts.commits += 1,
        }
    }
    Ok(counts)
}

fn main() -> Result<()> {
    println!("# Persistent-tree structural growth");
    println!();
    println!("N={N}, R={R}, payload={PAYLOAD_BYTES} bytes, globally unique edits");
    println!();
    println!("| k | blobs | branches | leaves | commits | total objects | canonical bytes | loose upper bound |");
    println!("| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
    for k in [1, 10, 100] {
        let counts = run(k)?;
        let total = counts.blobs + counts.branches + counts.leaves + counts.commits;
        // Initial graph is at most 18N+1; each commit adds at most 18k+1.
        let bound = 18 * N + 1 + R * (18 * k + 1);
        println!(
            "| {k} | {} | {} | {} | {} | {total} | {} | {bound} |",
            counts.blobs, counts.branches, counts.leaves, counts.commits, counts.bytes
        );
    }
    Ok(())
}
