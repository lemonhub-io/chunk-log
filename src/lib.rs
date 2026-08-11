//! chunklog is a version-control library for voxel worlds.
//!
//! Inspired by Git's object model, it provides content-addressed storage,
//! deduplication, and commit history for chunk data, decoupled from any
//! game engine.
//!
//! # Architecture
//!
//! - [`object`] – the typed canonical object model: blobs, persistent tree
//!   nodes, commits, [`struct@Hash`] and helpers.
//! - [`store`] – pluggable storage backends via the [`ObjectStore`] trait;
//!   [`SqliteStore`] is the transactional default and [`FilesystemStore`]
//!   retains the loose-file layout.
//! - [`repo`] – the high-level [`Repository`] API: init, commit, load,
//!   branches, checkout, diff, garbage collection, log.
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

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod diff;
#[cfg(not(target_arch = "wasm32"))]
pub mod gc;
pub mod object;
pub mod opfs;
#[cfg(not(target_arch = "wasm32"))]
pub mod repo;
pub mod store;

#[cfg(not(target_arch = "wasm32"))]
pub use diff::WorldDiff;
#[cfg(not(target_arch = "wasm32"))]
pub use gc::GcStats;
pub use object::{parse_hash, ChunkCoords, Commit, Hash, Object, TreeNode};
#[cfg(target_arch = "wasm32")]
pub use opfs::OpfsStore;
#[cfg(not(target_arch = "wasm32"))]
pub use repo::{Branch, ChangeSet, Checkout, LogEntry, Repository, World};
#[cfg(not(target_arch = "wasm32"))]
pub use store::{FilesystemStore, SqliteStore};
pub use store::{MemoryStore, ObjectStore};
