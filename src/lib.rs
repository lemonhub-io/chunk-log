//! chunklog is a version-control library for voxel worlds.
//!
//! It provides content-addressed storage, deduplication, and commit
//! history for chunk data, inspired by Git's object model.

#![forbid(unsafe_code)]

pub mod cli;
pub mod object;
pub mod repo;
pub mod store;

pub use object::{parse_hash, Commit, Hash, Object};
pub use repo::Repository;
pub use store::{FilesystemStore, ObjectStore};
