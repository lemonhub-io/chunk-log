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

- **Language**: Rust (1.70+)
- **Hashing**: XXH3 (fast, non‑cryptographic) for chunk content; SHA‑256 optional for integrity.
- **Compression**: flate2 (zlib) or zstd for blob storage.
- **Serialization**: Serde with bincode or postcard for object persistence.
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
| **diff** | Computes differences between two trees (changed chunks, added/removed). Used for incremental saving and `chunklog diff`. |
| **gc** | Garbage collection – traverses all reachable commits and deletes unreferenced blobs/trees. |
| **cli** | Command‑line subcommands: `init`, `commit`, `log`, `checkout`, `branch`, `diff`, `gc`. |

---

## 4. Data Flow

### 4.1 Save (Commit)
- Game provides a map of `(chunk_x, chunk_z) → compressed chunk data`.
- `Repository` builds a `Tree` object from these blobs (existing blobs are reused via hashing).
- Creates a `Commit` referencing the tree, with parent = current HEAD, timestamp, message.
- Writes commit object, updates `HEAD` to new commit hash.

### 4.2 Checkout
- Given a commit hash (or branch name), `Repository` reads its tree.
- Returns a list of chunk hashes and their coordinates.
- The game can then load only those blobs on demand (lazy loading).
- No file copying – just pointer change.

### 4.3 Branching
- Branch is a lightweight reference (file) pointing to a commit hash.
- `checkout -b new_branch` creates a new branch at current HEAD.
- `switch` changes the current branch reference.

---

## 5. Development Milestones

### v0.1.0 – Foundation (Core Storage + Commit)
- [x] Object definitions (Blob, Tree, Commit)
- [x] Filesystem object store (read/write by hash)
- [x] Repository init, commit creation
- [x] CLI: `init`, `commit -m`, `log` (basic)

> `chunklog commit` reads chunk files from `.chunklog/staging/` (each file named
> `<x>,<z>`), commits them, then clears the staging directory.

### v0.2.0 – Checkout & Branching
- [ ] Checkout (switch to any commit or branch)
- [ ] Branch creation, deletion, listing
- [ ] `HEAD` and refs management
- [ ] CLI: `checkout`, `branch`, `switch`

### v0.3.0 – Diff & Garbage Collection
- [ ] Tree diff (show changed chunks between two commits)
- [ ] Garbage collection (mark‑and‑sweep over reachable objects)
- [ ] CLI: `diff`, `gc`

### v1.0.0 – Stable API & Performance
- [ ] Full API documentation
- [ ] Benchmark suite (save/checkout times vs. naive copy)
- [ ] Integration example with a simple game (e.g., a minimal voxel renderer)
- [ ] CI/CD (GitHub Actions) with linting, tests, and coverage

### Future (v2.0)
- [ ] Merge / cherry‑pick across branches
- [ ] Remote synchronization (like `git push/pull`)
- [ ] Multiplayer collaboration with lock‑free merging

---

## 6. Testing Strategy

- **Unit tests**: Each module (object serialization, store read/write, diff logic) with mocked filesystem.
- **Integration tests**: End‑to‑end CLI tests: create repo, commit, checkout, verify chunks.
- **Performance benchmarks**: Measure commit and checkout time for worlds of varying size (100, 1000, 10000 chunks) against a baseline (full copy).
- **Fuzzing**: (optional) verify hash collisions are handled correctly.

---

## 7. Project Structure

```
chunklog/
├── Cargo.toml
├── LICENSE-MIT
├── LICENSE-APACHE
├── README.md
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
│       └── ...
├── tests/
│   ├── integration/
│   └── benchmarks/
└── examples/
    └── simple_game_integration.rs
```

---

## 8. Integration with Existing Engines

- **LemonCraft**: Replace current world save routine with `Repository::commit()`; on load, use `Repository::load_tree()` and fetch chunks on demand.
- **Veloren**: Similar approach; adapt to Veloren's chunk data structures.
- **Generic**: Provide a simple trait (`VoxelWorld`) that the library can work with, or just accept `HashMap<(i32,i32), Vec<u8>>`.

---

## 9. Licensing & Community

- **License**: MIT OR Apache‑2.0 – permissive, allowing use in both open‑source and commercial projects.
- **Community**: Aim to become a standard storage backend for voxel games. Encourage contributions via clear CONTRIBUTING.md and issue templates.
- **Documentation**: Host API docs on docs.rs after first release.

---

## 10. Risk Mitigation

- **Hash collision**: Use 256‑bit hash (SHA‑256) for production; XXH3 for speed in development.
- **Large commit history**: Provide `gc` and optional automatic pruning policies.
- **File corruption**: Write objects atomically (write to temp file then rename) and verify checksum on read.
- **Performance**: Early benchmarks ensure minimal overhead; consider memory mapping for large blobs.

---

## 11. Next Steps (Immediate)

1. Set up GitHub repository with the chosen name (`chunklog`).
2. Add `Cargo.toml` with dependencies (`serde`, `flate2`, `xxhash-rust`, `clap`).
3. Implement `object` module and `store` filesystem backend.
4. Write unit tests for store and serialization.
5. Implement `commit` command in CLI to verify end‑to‑end saving.

This plan keeps the project focused, modular, and deliverable in incremental milestones, while leaving room for future enhancements.
