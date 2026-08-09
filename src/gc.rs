//! Garbage collection types.
//!
//! The collection logic lives on
//! [`Repository::collect_garbage`](crate::Repository::collect_garbage).

/// Result of a garbage collection run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcStats {
    /// Number of unreachable objects deleted.
    pub removed: usize,
    /// Number of objects retained.
    pub retained: usize,
}
