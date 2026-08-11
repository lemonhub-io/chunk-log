# Changelog

## [Unreleased]

### Added

- Added a wasm-only `OpfsStore` backed by one append-only OPFS file, with
  committed-batch recovery, verified reads and initial-import batching.
- Added a reproducible Dedicated Worker/Chromium OPFS benchmark with raw sample
  archival and close/reopen verification.

### Changed

- Coalesced explicit OPFS batches into one contiguous write and one flush,
  removed the full-index clone at batch start, and retained the verified log as
  a coherent read cache.
- Removed the remaining large Wasm/JavaScript copies by passing synchronous
  access-handle reads and writes direct Wasm-memory views. Transactions now
  serialize into the retained log, avoid an ordering-only sort, and do not
  repeat an already-completed durable flush during close.
- Reused the immutable cache's verification-on-ingest invariant instead of
  rehashing each cached object on every direct `OpfsStore::read`; repository
  decoding still performs its independent address check.
- Replaced per-object pending payload allocations with one contiguous batch
  arena; final changes reference byte ranges and commit scans the arena
  sequentially without changing rollback or deduplication semantics.

### Performance

- On the documented Chromium/Windows host, batched N=10,000 import fell from
  5,069.7 ms to 39.0 ms and verified cached full read fell from 2,964.0 ms to
  13.5 ms. These are single-host object-layer medians.
- In a consecutive targeted N=10,000 comparison, zero-copy I/O and immutable
  cache reads reduced import from 45.7 ms to 29.3 ms, reopen from 22.6 ms to
  12.1 ms, and cached full read from 15.2 ms to 4.1 ms.
- A subsequent 10-sample batch-arena run measured 26.2 ms import, 12.6 ms
  reopen and 4.1 ms cached full read at N=10,000.

### Limitations

- OPFS deletion is logical until compaction is implemented, and the native
  `Repository` metadata layer is not yet available in browsers.

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

## [0.1.0]

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
