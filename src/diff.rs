//! World diff types.
//!
//! The diff logic lives on [`Repository::diff`](crate::Repository::diff).

use crate::object::{ChunkCoords, Hash};

/// The difference between two worlds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldDiff {
    /// Chunks present in `to` but not in `from`.
    pub added: Vec<(ChunkCoords, Hash)>,
    /// Chunks present in both worlds with different content, as
    /// `(old hash, new hash)` pairs.
    pub modified: Vec<(ChunkCoords, (Hash, Hash))>,
    /// Chunks present in `from` but not in `to`.
    pub removed: Vec<(ChunkCoords, Hash)>,
}

impl WorldDiff {
    /// The total number of changed chunks.
    pub fn len(&self) -> usize {
        self.added.len() + self.modified.len() + self.removed.len()
    }

    /// Whether the two worlds are identical.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
