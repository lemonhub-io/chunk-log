# chunklog Project Plan

## 1. Overview

chunklog is a standalone, version‑control library for voxel worlds, inspired by Git’s object model. It provides content‑addressed storage, deduplication, instant checkouts, and native branching for any voxel game engine.

**Primary Goals**:
- Decouple storage logic from game code (usable by LemonCraft, Veloren, Minetest, Godot, etc.).
- Offer a CLI for world management and a Rust crate for integration.
- Eliminate data duplication and corruption through immutable commits.
- Keep operations fast (O(1) checkout, incremental saving).

---

## 2. Technology Stack

- **Language**: Rust (1.85+)
- **Hashing**: SHA‑256 for all content hashing.
- **Compression**: flate2 (zlib) or zstd for blob storage (deferred; games currently store their own compressed chunk bytes).
- **Serialization**: Serde with bincode for object persistence.
- **Concurrency**: Tokio or async‑std for future network features; currently sync with standard library.
- **CLI**: clap for command‑line argument parsing.
- **Testing**: Rust’s built‑in test harness + criterion for benchmarks.

---

## 3. Core Modules

| Module | Responsibility |
| :----- | :-------------- |
| **object** | Defines `Blob` (compressed chunk data), `Tree` (mapping of chunk coordinates to blob hashes), `Commit` (metadata + root tree hash). Implements serialization/deserialization. |
| **store** | `ObjectStore` – a key‑value store (filesystem backend initially) mapping hash → object bytes. Handles read/write, deduplication (write only if missing). |
| **repo** | `Repository` – high‑level interface: open/init, head management, commit creation, checkout, branch operations. |
| **diff** | `WorldDiff` – added/modified/removed chunks between two commits (`Repository::diff`). Used for incremental saving and `chunklog diff`. |
| **gc** | `GcStats` – mark-and-sweep over objects reachable from HEAD and all branch refs (`Repository::collect_garbage`). |
| **cli** | Command‑line subcommands: `init`, `commit`, `log`, `checkout`, `branch`, `diff`, `gc`. |

---

## 4. Data Flow

### 4.1 Save (Commit)
- Game provides a map of `(chunk_x, chunk_z) → compressed chunk data`.
- `Repository` builds a `Tree` object from these blobs (existing blobs are reused via hashing).
- Creates a `Commit` referencing the tree, with parent = current HEAD, timestamp, message.
- Writes commit object, updates `HEAD` to new commit hash.

### 4.2 Checkout
- Given a commit hash (or branch name), `Repository::checkout` moves `HEAD`
  and the branch reference only — no data is copied.
- World data is loaded separately: `Repository::load(commit)` returns all
  chunks eagerly, or `Repository::chunk_hashes(commit)` lists
  `(coords, blob hash)` pairs so the game can fetch blobs on demand (lazy loading).

### 4.3 Branching
- A branch is a lightweight reference (file under `refs/heads/`) pointing to a commit hash.
- `checkout -b new_branch` creates a new branch at current HEAD and switches to it.
- `checkout` switches the current branch reference; checking out a commit
  hash yields a detached HEAD.

---

## 5. Development Status

> **Versioning note**: This plan originally labeled each stage `v0.1.0` /
> `v0.2.0` / `v0.3.0` / `v1.0.0`. That evolution scheme is **overturned**:
> all scope below is completed in one cycle and ships as release
> **v0.1.0**.

### Foundation – Core Storage + Commit
- [x] Object definitions (Blob, Tree, Commit)
- [x] Filesystem object store (read/write by hash)
- [x] Repository init, commit creation
- [x] CLI: `init`, `commit -m`, `log` (basic)

> `chunklog commit` reads chunk files from `.chunklog/staging/` (each file named
> `<x>,<z>`), commits them, then clears the staging directory.

### Checkout & Branching
- [x] Checkout (switch to any commit or branch, including detached HEAD)
- [x] Branch creation, deletion, listing
- [x] `HEAD` and refs management (symbolic HEAD: `ref: refs/heads/<name>`)
- [x] CLI: `checkout` (with `-b`), `branch`

> `switch` was merged into `checkout` (`checkout -b` creates and switches);
> a separate command would duplicate functionality.

### Diff & Garbage Collection
- [x] Tree diff (show changed chunks between two commits)
- [x] Garbage collection (mark-and-sweep over reachable objects)
- [x] CLI: `diff`, `gc`

### Release v0.1.0 – Stable API & Performance
- [x] Full API documentation (rustdoc, enforced by `missing_docs`)
- [x] Hash upgrade to SHA‑256
- [x] Benchmark suite (commit/load/checkout vs. naive copy)
- [x] Integration example with a simple game (`examples/simple_game_integration.rs`)
- [x] CI/CD (GitHub Actions) with linting, tests, and coverage

### Future
- [ ] Merge / cherry‑pick across branches
- [ ] Remote synchronization (like `git push/pull`)
- [ ] Multiplayer collaboration with lock‑free merging
- [ ] Delta compression for chunk blobs
- [ ] Automatic pruning policies for commit history

---

## 6. Testing Strategy

- **Unit tests**: Each module (object serialization, store read/write, diff logic) with mocked filesystem.
- **Integration tests**: End‑to‑end tests: create repo, commit, checkout, branches, diff, gc, verify chunks.
- **Performance benchmarks**: `cargo bench` – commit, load, and checkout for worlds of 100/1000/10000 chunks against a naive full-copy baseline.
- **CI/CD**: GitHub Actions – formatting, clippy (`-D warnings`), tests, doctests, docs, MSRV (1.85) check, and coverage.

---

## 7. Project Structure

```
chunklog/
├── Cargo.toml
├── LICENSE-MIT
├── LICENSE-APACHE
├── README.md
├── .github/workflows/ci.yml
├── src/
│   ├── lib.rs
│   ├── main.rs
│   ├── object.rs
│   ├── store.rs
│   ├── repo.rs
│   ├── diff.rs
│   ├── gc.rs
│   └── cli/
│       ├── mod.rs
│       ├── init.rs
│       ├── commit.rs
│       ├── log.rs
│       ├── branch.rs
│       ├── checkout.rs
│       ├── diff.rs
│       └── gc.rs
├── benches/
│   └── storage.rs
├── tests/
│   └── integration/
└── examples/
    └── simple_game_integration.rs
```

---

## 8. Integration with Existing Engines

- **LemonCraft**: Replace current world save routine with `Repository::commit()`; on load, use `Repository::load()` or `Repository::chunk_hashes()` and fetch chunks on demand.
- **Veloren**: Similar approach; adapt to Veloren's chunk data structures.
- **Generic**: `World` is `HashMap<(i32,i32), Vec<u8>>` – chunk coordinates to compressed chunk data. See `examples/simple_game_integration.rs` for a complete walkthrough.

---

## 9. Licensing & Community

- **License**: MIT OR Apache‑2.0 – permissive, allowing use in both open‑source and commercial projects.
- **Community**: Aim to become a standard storage backend for voxel games. Encourage contributions via clear CONTRIBUTING.md and issue templates.
- **Documentation**: Host API docs on docs.rs after first release.

---

## 10. Risk Mitigation

- **Hash collision**: 256‑bit SHA‑256 for all content hashing.
- **Large commit history**: `chunklog gc` (mark-and-sweep) with optional automatic pruning policies planned.
- **File corruption**: Objects are written atomically (temp file + rename).
- **Performance**: Benchmark suite (`cargo bench`) tracks commit/load/checkout overhead; memory mapping for large blobs considered if needed.

---

## 11. Status

All planned scope is implemented and covered by tests; the crate is
ready for its first release as **v0.1.0**. Remaining ideas are tracked
under *Future* in section 5.
