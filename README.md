# chunklog

**Version control for voxel worlds** — a standalone Rust library and CLI,
inspired by Git's object model, that treats a voxel world's chunks like
files in a repository.

`World` (chunk coordinates → compressed chunk data) goes in, commit history
comes out. Any voxel engine can use it — LemonCraft, Veloren, Minetest,
Godot, or your own.

## Features

- **Content-addressed storage** — chunk data is addressed by its BLAKE3
  hash, so identical chunks are stored once, automatically.
- **Incremental saves** — a commit of a mostly unchanged world only writes
  the chunks that changed.
- **Instant checkout** — switching versions is a reference move (O(1));
  world data is loaded on demand.
- **Native branching** — experiment on a branch, roll back, or delete
  branches and let `gc` reclaim their objects.
- **Pluggable storage** — `ObjectStore` trait with a filesystem backend;
  memory or network backends are trivial to add.
- **No engine coupling** — the library never touches game state; it accepts
  and returns chunk bytes.
- **CLI + library** — the `chunklog` command line for world management, or
  the crate for direct integration.

## Status

Core workflow complete and covered by tests: init, commit, load, branches,
checkout, diff, garbage collection. **0.1.0** is the first release.

> The original plan labeled development stages v0.1.0 → v1.0.0; that scheme
> was abandoned and all scope ships as v0.1.0.

## Getting started

### As a library

```toml
[dependencies]
chunklog = "0.1"
```

```rust
use chunklog::Repository;
use std::collections::HashMap;

// A world is chunk coordinates -> compressed chunk bytes.
let mut world = HashMap::new();
world.insert((0, 0), vec![1, 2, 3]);

let mut repo = Repository::init("world.chunklog")?;
let commit = repo.commit(&world, "initial save")?;

// ... later, restore the world of any commit:
let restored = repo.load(commit)?;
assert_eq!(restored, world);
```

See `examples/simple_game_integration.rs` for a complete game workflow
(save, incremental save, rollback, branching, gc):

```sh
cargo run --example simple_game_integration
```

### CLI

```sh
cargo install chunklog
```

```sh
cd myworld
chunklog init                       # create .chunklog/
# drop chunk files named "<x>,<z>" into .chunklog/staging/
chunklog commit -m "explored the plains"
chunklog log
chunklog branch                     # list branches (* = current)
chunklog checkout -b experiment     # create and switch
chunklog checkout main              # switch back
chunklog diff                       # world changes vs. empty world
chunklog diff main experiment       # or between two commits
chunklog gc                         # delete unreachable objects
```

| Command | Description |
| --- | --- |
| `init` | Initialize a repository (default branch `main`) |
| `commit -m <msg>` | Commit chunk files staged in `.chunklog/staging/` (each named `<x>,<z>`) |
| `log` | Show commit history |
| `branch` | List branches; `branch <name>` creates; `branch -d <name>` deletes |
| `checkout <target>` | Switch to a branch or commit hash; `-b` creates a new branch |
| `diff [from] [to]` | Show added/modified/removed chunks (defaults: empty world → HEAD) |
| `gc` | Remove objects unreachable from HEAD and all branches |

## How it works

### Object model

Three kinds of objects live in `.chunklog/objects/`, each addressed by the
BLAKE3 hash of its content:

- **Blob** — raw chunk bytes (as provided by the game, e.g. compressed).
- **Tree** — mapping of chunk coordinates to blob hashes.
- **Commit** — root tree hash, parent hash, timestamp, message.

Because hashes are content-derived, identical chunks produce identical
blobs and are written only once — deduplication is a property of the
addressing scheme, not a separate feature.

### References

- `HEAD` is symbolic: `ref: refs/heads/main` on a branch, a bare commit
  hash when detached.
- Branches are files in `refs/heads/` pointing at commit hashes.

### Checkout

`checkout` only moves references — it never copies data. World data is
materialized separately: `load(commit)` returns the full world, or
`chunk_hashes(commit)` lists `(coords, blob hash)` pairs so the game
fetches only the chunks it needs.

### Storage backends

`ObjectStore` defines a minimal byte-level contract (`read`, `write`,
`list`, `delete`). The default `FilesystemStore` writes objects atomically
(temp file + rename). Custom backends (memory, network, cloud) implement
the trait and are plugged in via `Repository::init_with` / `open_with`.

### Integrating with a game

1. Keep your world as `HashMap<(i32, i32), Vec<u8>>` (chunk coordinates →
   compressed chunk bytes).
2. On save: `repo.commit(&world, message)`.
3. On load: `repo.load(commit)` (eager) or `repo.chunk_hashes(commit)` +
   `store.read(hash)` (lazy).

Blobs are stored exactly as provided; decompression is the game's job.

## Performance

Indicative figures from the criterion suite (`cargo bench`) on a desktop;
chunk payload = 256 bytes:

| Operation | 100 chunks | 1,000 chunks | 10,000 chunks |
| --- | ---: | ---: | ---: |
| commit | ~8 ms | ~38 ms | ~310 ms |
| load (full world) | ~3 ms | ~21 ms | ~200 ms |
| checkout (reference move) | ~1.2 ms | ~1.3 ms | ~1.9 ms |
| naive full copy (baseline) | ~14 ms | ~255 ms | ~2 s |

Checkout is constant-time regardless of world size; commit time grows with
the number of *changed* chunks, not with history length.

## Roadmap

- [ ] Merge / cherry-pick across branches
- [ ] Remote synchronization (like `git push/pull`)
- [ ] Multiplayer collaboration with lock-free merging
- [ ] Delta compression for chunk blobs

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
cargo doc --no-deps
cargo bench
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the contribution guide,
[CHANGELOG.md](CHANGELOG.md) for release notes, and [SECURITY.md](SECURITY.md)
for security reporting.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE),
at your option.
