//! The object model: content-addressed objects and hashes.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_64;

/// A content hash (XXH3 64-bit).
///
/// Hashes identify objects in a store and are derived from the object's
/// serialized bytes, which makes them deterministic and content-addressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Hash(
    /// The raw hash bytes.
    pub [u8; 8],
);

/// Chunk coordinates in a voxel world.
pub type ChunkCoords = (i32, i32);

/// An immutable, structured object in the store.
///
/// Chunk blobs are *not* part of this enum: they are stored as raw bytes
/// directly in an [`ObjectStore`](crate::ObjectStore), and trees reference
/// them by hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Object {
    /// Mapping of chunk coordinates to blob hashes.
    Tree(BTreeMap<ChunkCoords, Hash>),
    /// A commit referencing a root tree.
    Commit(Commit),
}

/// Metadata for a single commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    /// Hash of the root tree of this commit.
    pub tree: Hash,
    /// Hash of the parent commit, or `None` for the first commit.
    pub parent: Option<Hash>,
    /// Unix timestamp (seconds) of the commit.
    pub timestamp: u64,
    /// Commit message.
    pub message: String,
}

impl Object {
    /// Computes the content hash of this object.
    pub fn hash(&self) -> Hash {
        Hash(xxh3_64(&self.to_bytes()).to_le_bytes())
    }

    /// Serializes this object to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).expect("serialization of an object cannot fail")
    }

    /// Deserializes an object from bytes.
    pub fn from_bytes(bytes: &[u8]) -> anyhow::Result<Self> {
        Ok(bincode::deserialize(bytes)?)
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Parses a 16-character hex string into a `Hash`.
pub fn parse_hash(s: &str) -> anyhow::Result<Hash> {
    if s.len() != 16 {
        anyhow::bail!("invalid hash: {s}");
    }
    let mut hash = [0u8; 8];
    for (i, chunk) in s.as_bytes().chunks_exact(2).enumerate() {
        hash[i] = u8::from_str_radix(std::str::from_utf8(chunk)?, 16)?;
    }
    Ok(Hash(hash))
}
