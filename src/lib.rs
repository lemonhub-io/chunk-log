//! chunklog is a version-control library for voxel worlds.
//!
//! Inspired by Git's object model, it provides content-addressed storage,
//! deduplication, and commit history for chunk data, decoupled from any
//! game engine.
//!
//! # Architecture
//!
//! - [`object`] – the object model: [`Object`] (trees, commits),
//!   [`struct@Hash`] and helpers. Chunk blobs are raw bytes in the store.
//! - [`store`] – pluggable storage backends via the [`ObjectStore`] trait,
//!   with a [`FilesystemStore`] implementation.
//! - [`repo`] – the high-level [`Repository`] API: init, commit, load,
//!   branches, checkout, log.
//! - [`cli`] – the `chunklog` command-line tool.
//!
//! # Example
//!
//! ```
//! use chunklog::Repository;
//!
//! # let dir = std::env::temp_dir().join("chunklog-crate-doc-example");
//! # let _ = std::fs::remove_dir_all(&dir);
//! let mut repo = Repository::init(&dir)?;
//!
//! let mut world = std::collections::HashMap::new();
//! world.insert((0, 0), vec![1, 2, 3]);
//! let commit = repo.commit(&world, "initial save")?;
//!
//! assert_eq!(repo.head(), Some(commit));
//! assert_eq!(repo.log()?.len(), 1);
//! # let _ = std::fs::remove_dir_all(&dir);
//! # Ok::<(), anyhow::Error>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod cli;
pub mod object;
pub mod repo;
pub mod store;

pub use object::{parse_hash, ChunkCoords, Commit, Hash, Object};
pub use repo::{Branch, Checkout, LogEntry, Repository, World};
pub use store::{FilesystemStore, ObjectStore};
