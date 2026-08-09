# chunklog

Content-addressed version control for coordinate-addressed world state.

`chunklog` is a Rust library and CLI for versioning voxel chunks or any state that can be represented as `(i32, i32) → bytes`. It stores immutable, typed BLAKE3-addressed objects and uses a persistent coordinate radix tree so an incremental commit republishes only affected paths.

## Status

Version 0.3.0 is a research implementation. Core snapshot, incremental commit, load, diff, branch, logical checkout and garbage-collection workflows are implemented and tested. New repositories use a transactional SQLite content-addressed store; unsupported older formats are rejected instead of being silently interpreted.

The format-2 benchmark suite and controlled Luanti integration artifact are archived under `paper-results/`. On the documented Windows/NTFS host, an N=1,000 initial snapshot fell from 10.427 s with loose objects to 47.433 ms with SQLite, about 220× faster.

## Properties

- **Typed content addressing:** Blob, Tree branch, Tree leaf and Commit objects have distinct canonical tags and BLAKE3 addresses.
- **Verified reads:** object bytes are rehashed on read; silent blob or metadata corruption is rejected.
- **Persistent coordinate tree:** changes copy only affected paths through a fixed-depth radix Merkle tree.
- **Two commit modes:** full snapshots have Θ(N) scanning cost; change sets update only explicitly edited coordinates.
- **Logical checkout:** switching HEAD does not materialize world payloads.
- **Incremental CLI staging:** staged files are upserts and `.remove` contains explicit removals.
- **Single-writer safety:** mutating operations use a repository lock and references use atomic replacement.
- **Transactional default object writes:** all objects for one commit enter SQLite in one transaction before its ref is published.
- **Fail-closed marking:** GC verifies every reachable object before deletion begins; SQLite sweep is transactional, while generic stores retain backend-specific semantics.
- **Pluggable object storage:** custom backends implement the byte-level `ObjectStore` trait.

## Library usage

```rust
use chunklog::{ChangeSet, Repository};
use std::collections::HashMap;

# let dir = tempfile::tempdir()?;
let mut repo = Repository::init(dir.path())?;

let world = HashMap::from([
    ((0, 0), vec![1, 2, 3]),
    ((1, 0), vec![4, 5, 6]),
]);
let initial = repo.commit_snapshot(&world, "initial world")?;

let mut changes = ChangeSet::new();
changes.upsert((0, 0), vec![9, 9, 9]);
changes.remove((1, 0));
changes.upsert((2, 0), vec![7]);
let edited = repo.commit_changes(&changes, "localized edit")?;

assert_eq!(repo.load(initial)?, world);
assert_eq!(repo.load(edited)?.len(), 2);
# Ok::<(), anyhow::Error>(())
```

`Repository::commit` remains an alias for `commit_snapshot` for compatibility. It traverses the complete input world and is not an O(k) API. Callers that know their edited coordinates should use `commit_changes`.

For lazy loading, call `chunk_hashes(commit)` and then `read_chunk(blob_hash)`. Direct `ObjectStore::read` returns canonical encoded object bytes, not a decoded chunk payload.

## CLI

```text
chunklog init

# Upsert two chunks in the next commit:
#   .chunklog/staging/0,0
#   .chunklog/staging/1,-2

# Explicitly remove coordinates, one per line:
#   .chunklog/staging/.remove
#   4,5
#   -3,9

chunklog commit -m "edited spawn"
chunklog log
chunklog branch experiment
chunklog checkout experiment
chunklog checkout main
chunklog diff main experiment
chunklog gc
```

Staging is an incremental patch. Unmentioned coordinates remain unchanged. A coordinate may not be both an upsert file and an entry in `.remove`.

| Command | Description |
| --- | --- |
| `init` | Initialize a format-2 SQLite repository on unborn branch `main` |
| `commit -m <msg>` | Apply the staged upserts and removals to HEAD |
| `log` | Walk first-parent history from HEAD |
| `branch [name]` | List or create branches |
| `branch -d <name>` | Delete a non-current branch |
| `checkout <target>` | Move HEAD to a branch or full commit hash without loading chunks |
| `checkout -b <name>` | Create and switch to a branch |
| `diff [from] [to]` | List added, modified and removed coordinates |
| `gc` | Delete objects unreachable from HEAD and all branches |

## Object model

Format 2 uses canonical object wire version 1 with four object forms:

- **Blob:** a length-prefixed opaque chunk payload;
- **Tree branch:** a sorted sparse map from one coordinate nibble to child address;
- **Tree leaf:** an exact coordinate and Blob address;
- **Commit:** a root Tree address, optional parent, timestamp and message.

Coordinates occupy eight bytes (`x` then `z`) and are traversed as sixteen nibbles. A `k`-coordinate change affects the union of at most `16k` root-to-leaf paths; common prefixes reduce the actual number of new Tree nodes. This is different from the former flat Tree, which rewrote Θ(N) metadata on every save.

The complete durable specification is in [FORMAT.md](FORMAT.md).

## Cost model

Let N be the number of coordinates and k the number of explicit changes.

| Operation | Structural work | Payload work |
| --- | --- | --- |
| full snapshot commit | Θ(N) traversal and Tree construction | hashes N payloads |
| incremental change-set commit | O(k·16) radix paths, less with shared prefixes | hashes k upserts |
| logical checkout | independent of N | reads no Blob or Tree |
| full load | Θ(N) Tree/Blob traversal | reads all N payloads |
| diff (current implementation) | Θ(N₁ + N₂) materialized hash maps | reads no Blob payloads |
| GC | linear in all reachable and stored objects | verifies reachable payload objects |

Absolute performance depends strongly on storage backend, filesystem, payload size and integrity-verification policy. The archived single-host medians for the default SQLite store are:

| Operation | N=100 | N=1,000 | N=10,000 |
| --- | ---: | ---: | ---: |
| initial full snapshot | 14.021 ms | 47.433 ms | 1.156 s |
| k=1 incremental commit | 10.446 ms | 13.577 ms | — |
| full load | 21.221 ms | 222.167 ms | — |

Use `cargo bench` for the target machine; these measurements do not establish cross-platform absolute performance.

## Consistency and recovery boundaries

- Objects are immutable. The default backend commits one SQLite object transaction before publishing the reference.
- Reference replacement is atomic at the file level.
- Mutating operations are serialized by `.chunklog/LOCK`.
- A process crash may leave unreachable objects or a stale lock; it cannot justify deleting a lock while another writer may still be active.
- SQLite uses rollback journaling and `synchronous=FULL`; the SQLite transaction and filesystem ref replacement are still not one cross-file transaction.
- GC performs no deletion after a marking error.
- SQLite GC deletes are one transaction. Loose/custom stores may leave a partial sweep and must be safe to retry.

## Development and reproduction

```text
cargo test
cargo test --all-targets --no-run
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
cargo doc --no-deps
cargo bench
```

`CLAIMS_EVIDENCE.md` tracks paper claims against code and evidence. `REPAIR_PLAN.md` records the remediation plan, and `REPAIR_AUDIT.md` records its final implementation and verification audit. The Luanti workload can be reproduced through `paper-workloads/run-luanti.ps1`.

## Limitations

- Single writer only; stale locks require operator confirmation before removal.
- No merge, cherry-pick, remote synchronization or multiplayer protocol.
- Diff currently expands both trees instead of recursively skipping shared roots.
- No migration command for the former unversioned or format-1 loose repository layout.
- The Luanti artifact uses controlled singlenode generation, not a production-player edit history.

## License

Licensed under either [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
