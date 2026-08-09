# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Content-addressed object store with SHA-256 hashing and automatic
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
