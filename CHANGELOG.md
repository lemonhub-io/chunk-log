# Changelog

## [0.3.0] - 2026-08-09

### Added

- Added a verified SQLite CAS backed by one database file and explicit object-write batches.
- Added `Repository::<FilesystemStore>::init_loose` and `open_loose` for loose-file compatibility and performance comparisons.

### Changed

- New repositories now use `SqliteStore` by default and write every commit's objects in one SQLite transaction before publishing its ref.
- Bumped the repository marker to format 2; format-1 loose repositories are rejected rather than silently opened with the wrong backend.
- GC deletion is transactional on `SqliteStore`; custom and loose stores retain their documented backend-specific semantics.

### Performance

- On the documented Windows/NTFS host, the N=1,000 initial snapshot median fell from 10.427 s with loose files to 47.433 ms with SQLite (about 220× faster).
- The SQLite initial-snapshot median was 1.156 s at N=10,000.

## [0.2.0] - 2026-08-09

### Changed

- Replaced the flat postcard Tree with a versioned, typed canonical object format and a persistent 16-level coordinate radix Merkle tree.
- Added `commit_snapshot` and incremental `commit_changes`; `commit` remains a full-snapshot compatibility alias.
- Changed CLI staging to incremental patch semantics with `.remove` tombstones.
- Added an explicit `.chunklog/FORMAT` marker; unversioned experimental repositories are rejected.
- Updated lazy chunk access to `Repository::read_chunk` because store bytes are now typed canonical objects.

### Security

- Validate every branch-name entry point and enforce refs-directory containment.
- Verify object hashes on reads and when reusing existing filesystem objects.
- Use domain-separated Blob, Tree and Commit encodings.
- Serialize repository mutations with a lock and atomically replace references.

### Fixed

- Corrected the benchmark generator so it produces exactly N unique payloads.
- Corrected checkout and naive-copy benchmark setup.
- Removed the unsupported claim that GC sweep is all-or-nothing; marking is fail-closed and sweep is retryable.

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Content-addressed object store with BLAKE3 hashing and automatic
  deduplication of chunk blobs.
- Git-inspired object model: `Blob` (raw chunk bytes), `Tree`
  (coordinate -> hash mapping), `Commit` (root tree, parent, timestamp,
  message).
- Filesystem object store with atomic writes (temp file + rename), plus
  a pluggable `ObjectStore` trait for custom backends.
- Repository operations: init, open, commit, load (eager) and
  `chunk_hashes` (lazy loading), branches (create/list/delete), checkout
  (branch or detached commit), diff (added/modified/removed chunks),
  garbage collection (mark-and-sweep from HEAD and all refs).
- Symbolic HEAD and `refs/heads` layout with `main` as the default
  branch.
- CLI: `init`, `commit`, `log`, `branch`, `checkout`, `diff`, `gc`.
- Full rustdoc API documentation with `missing_docs` enforcement.
- Criterion benchmark suite (commit/load/checkout vs. naive copy).
- Integration example: `examples/simple_game_integration.rs`.
- CI/CD via GitHub Actions (fmt, clippy, tests, doctests, docs, MSRV,
  coverage).
- MIT OR Apache-2.0 licensing.
