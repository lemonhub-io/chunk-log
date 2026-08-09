# chunklog: Content-Addressed Version Control for Voxel Game Worlds

## Abstract

Voxel games generate and mutate large, long-lived worlds, yet their
persistence systems rarely provide versioning: saves are either full
copies of the world or engine-specific formats without history, rollback,
or branching. This paper presents chunklog, a standalone version-control
library and CLI for voxel worlds that adapts Git's content-addressed
object model to chunk-based data. A world is modeled as a map of chunk
coordinates to opaque byte payloads; chunks are addressed by the SHA-256
hash of their content, so deduplication emerges from the addressing
scheme rather than a separate feature. Commit is incremental by
construction, checkout moves only references (O(1)) and defers data
materialization to the consumer, and a byte-level storage abstraction
decouples object management from any particular backend. We evaluate the
implementation with a 33-test suite spanning both filesystem and
in-memory storage, and with a criterion benchmark suite. Results show
constant-time checkout independent of world size, commit and load
throughput of roughly 30–50 thousand chunks per second, and a 6.2x
reduction in save time over naive full copies at 10,000 chunks; storage
requirements grow with edited chunks rather than save count. We conclude
with a discussion of limitations, notably the absence of delta
compression at sub-chunk granularity.

## 1. Introduction

Voxel worlds are among the most data-intensive artifacts in interactive
entertainment. A modest Minecraft-style world may contain tens of
thousands of chunks — each holding a 16x16 column of block data — and
these chunks change continuously as players build, dig, and modify the
terrain. Long-lived worlds additionally face a demand that ordinary save
files do not: *history*. Players and server operators want to undo
building mistakes, roll back griefing, restore regions corrupted by
crashes, or experiment with a change and later discard it. Traditional
approaches are unsatisfactory in three ways:

1. **Full copies are wasteful.** Backup scripts that copy the world
   directory replicate every chunk on every snapshot, and storage grows
   linearly with the number of saves regardless of how little changed.
2. **Engine-specific formats lock in the consumer.** Minetest stores
   worlds in SQLite databases and Veloren in custom binary layouts;
   versioning logic cannot be reused across engines.
3. **History is usually absent.** Even when backups exist, they are flat
   snapshots: there is no notion of a causal chain of saves, of branches,
   or of meaningful diffs between points in time.

The observation driving this work is that Git's object model already
solves the general problem of "track a tree of mutable binary content
with cheap incremental history" — and that a voxel world is exactly such
a tree, with chunks in the role of files. This paper describes
**chunklog**, a standalone Rust library and command-line tool that
applies that model to voxel worlds with minimal assumptions about the
game that hosts it.

Our contributions are:

- **An object model for voxel worlds.** Blobs (raw chunk bytes), trees
  (chunk-coordinate to hash mappings), and commits form a content-
  addressed graph. Deduplication is a property of addressing, not a
  separate subsystem.
- **O(1) checkout.** Switching between saved states moves only
  references; world data is materialized lazily by the consumer through
  an explicit load or hash-listing interface.
- **A byte-level storage abstraction.** An `ObjectStore` trait with
  `read`, `write`, `list`, and `delete` decouples the object layer from
  any concrete backend, and makes the entire system testable against an
  in-memory store.
- **Evaluation.** A 33-test suite and a criterion benchmark show
  constant-time checkout, ~30K–50K chunks/s commit/load throughput, and
  a 6.2x save-time improvement over naive full copies at 10,000 chunks.

## 2. Background and Related Work

### 2.1 Git's Object Model

Git represents a repository as a directed acyclic graph of immutable,
content-addressed objects [1]. *Blobs* hold raw file bytes, *trees* map
names to hashes, and *commits* reference a root tree plus metadata and a
parent chain. Every object is addressed by the hash of its serialized
content, which yields three properties that chunklog inherits directly:
*deduplication* (identical content shares one object), *integrity* (the
hash is a verifiable digest of the content), and *structural sharing*
(commits with overlapping trees reuse the same objects, enabling O(diff)
history rather than O(world) per commit).

### 2.2 Content-Addressed Storage Beyond Git

The content-addressing idea has been generalized far beyond version
control. IPFS addresses files by their hashes in a peer-to-peer
filesystem [2]. casync [3] and OSTree [4] apply content addressing to
operating-system images, splitting large trees into independently
addressable blobs to support incremental updates. Container registries
use layered content-addressable storage for the same reason. chunklog
occupies the same design space applied to a specific domain — voxel game
worlds — where the natural unit of content is the *chunk*, and where the
consumers are interactive programs rather than package managers.

### 2.3 World Persistence in Voxel Games

Voxel engines persist worlds in engine-specific formats. Minecraft uses
region files (the Anvil format) that batch chunks into bounded files [5].
Minetest stores world data in SQLite databases [6]. Veloren uses custom
binary chunk archives [7]. These formats are optimized for a single
engine's read/write patterns; none of them provides version history or
branching. Community tooling — MCEdit, WorldEdit, and myriad backup
scripts — either edits worlds in place or snapshots them wholesale.
chunklog deliberately sits above this layer: it treats chunk payloads as
opaque bytes, so it is compatible with any engine that can serialize a
chunk to a byte string, while adding a version-control dimension that
existing formats lack.

### 2.4 Related Versioning Systems

Several systems version game saves without being engine-independent.
Save-slot systems (quicksave, autosave rotations) bound the number of
snapshots but discard history. Whole-directory sync tools (rsync
snapshots, btrfs snapshots) provide cheap copies but no semantic model
of the world, no branches, and no diff at the chunk level. To the best
of our knowledge, no existing library offers Git-style version control
as a generic, engine-agnostic service for voxel worlds; the closest
analogues are generic content-addressed sync tools applied ad hoc.

## 3. Design

### 3.1 Design Goals

The design is governed by five requirements:

1. **Engine agnosticism.** The library must never interpret chunk
   payloads. It accepts and returns bytes and imposes only a coordinate
   convention `(x, z)`.
2. **Incremental saves.** Saving a mostly unchanged world must cost
   roughly the *edited* chunks, not the world size.
3. **Instant switching.** Moving between saved states must not copy data.
4. **Pluggable persistence.** The object storage backend must be
   replaceable (filesystem today, network or cloud tomorrow).
5. **Fail loudly.** Corruption and violated invariants must surface as
   errors, never as silent data loss.

### 3.2 World Abstraction

The unit of interaction is the *World*: a map from chunk coordinates to
opaque payload bytes, `HashMap<(i32, i32), Vec<u8>>`. A chunk is the
atomic unit of persistence and deduplication — the analogue of a Git
file. The payload is assumed to be the engine's own serialization of the
chunk (typically compressed); chunklog neither compresses nor
decompresses it. This keeps the library trivially integrable: any engine
that can materialize a chunk to bytes can participate.

### 3.3 Object Model

chunklog stores three kinds of objects under a common content hash
(SHA-256 [8], chosen over fast non-cryptographic hashes for the
integrity guarantees the versioning use case calls for):

- **Blob** — the raw payload bytes of one chunk. Blobs are stored as-is;
  they are the leaves of the object graph and never need to be parsed.
- **Tree** — a mapping from chunk coordinates to blob hashes. Trees are
  serialized with bincode over a `BTreeMap`, which yields a canonical,
  sorted encoding so that identical chunk sets always produce identical
  tree hashes — a precondition for structural sharing between commits.
- **Commit** — a root tree hash, an optional parent hash, a timestamp,
  and a message.

The object graph is exactly Git's: `commit → tree → blobs`, with
commits chained by parent pointers. Because addresses are content
derived, two commits of identical worlds share the same tree and blobs;
a commit of a world where one chunk changed reuses *all* other objects.
Deduplication is therefore not an optimization layered on top of the
system — it is the identity of the system.

### 3.4 Repository Layout and References

A repository is a directory with the following layout:

```
.chunklog/
├── objects/          # content-addressed object store (files named by hex hash)
├── refs/heads/       # branch files: name → commit hash
├── HEAD              # symbolic ref: "ref: refs/heads/<branch>" or a bare hash
└── staging/          # CLI staging area: files named "<x>,<z>"
```

`HEAD` follows Git's semantics. On a branch it is a symbolic reference
(`ref: refs/heads/main`); when the user checks out a bare commit hash it
becomes detached. A branch whose reference file is absent is *unborn*
— a valid state for a freshly initialized repository whose default
branch is `main`.

### 3.5 Operations

All high-level operations are methods on a `Repository`:

- **commit** — for each chunk, write its bytes as a blob (a no-op when
  the hash already exists); build the canonical tree; write the commit
  chained to the current head; advance the current branch or `HEAD`.
  Cost is proportional to the number of *distinct* chunk payloads in the
  world, i.e., typically the number of edited chunks.
- **load** — resolve a commit to its tree and read every blob,
  reconstructing the `World`. Eager materialization for small worlds.
- **chunk_hashes** — return the `(coords, hash)` pairs of a commit's
  tree without reading blob contents. This is the interface for *lazy*
  materialization: a game fetches only the blobs it needs.
- **checkout** — resolve a branch name or commit hash and rewrite
  `HEAD`/refs. It moves references only; no bytes are copied. This is
  what makes switching constant-time (Section 5.2).
- **diff** — compare the trees of two commits (or a commit against an
  empty world) and produce added/modified/removed chunk lists. `from =
  None` makes the first commit's contents enumerable as a diff.
- **collect_garbage** — mark-and-sweep: traverse commits and trees
  reachable from `HEAD` and every branch ref, then delete all other
  objects from the store. Trees and commits are parsed strictly; blob
  hashes are marked reachable by reference without being read, so a
  corrupt blob can never cause silent deletion of reachable data.
- **resolve** — map a branch name or hash string to a commit hash.

### 3.6 Storage Abstraction

Object storage is defined by a four-method byte-level contract:

```rust
trait ObjectStore {
    fn read(&self, hash: Hash) -> Result<Vec<u8>>;
    fn write(&self, data: &[u8]) -> Result<Hash>;
    fn list(&self) -> Result<Vec<Hash>>;
    fn delete(&self, hash: Hash) -> Result<()>;
}
```

Two design choices deserve emphasis. First, the contract is *byte
level*: stores never parse objects, so a custom backend only handles
bytes, and the object layer owns all serialization. Second, `write`
derives the hash from the data, making content addressing a store
property; implementations must be idempotent for identical data, which
is what gives commit its incremental behavior on any backend. The
default `FilesystemStore` writes objects atomically (temp file +
rename), addressing the crash-consistency concern common to save
systems. The same trait makes the whole library testable against an
in-memory store (Section 5.1), which doubles as a reference behavior
check for custom backends.

## 4. Implementation

chunklog is implemented in Rust (edition 2021, MSRV 1.85) as a library
plus a thin CLI binary. Object serialization uses Serde with bincode;
hashing uses the SHA-256 implementation from the `sha2` crate; the CLI
uses clap. The library is `#![forbid(unsafe_code)]` and enforces full
API documentation with `#![warn(missing_docs)]`.

The source is organized into four layers that mirror the design:

- `object` — the object model: `Hash`, `Object` (`Tree`/`Commit`),
  serialization helpers.
- `store` — the `ObjectStore` contract and `FilesystemStore`.
- `repo` — `Repository`: layout, refs, and all operations of Section
  3.5.
- `cli` — subcommands `init`, `commit`, `log`, `branch`, `checkout`,
  `diff`, `gc`.

Blobs deliberately live outside the serialized `Object` enum: they are
raw store entries, and trees reference them by hash. This keeps blob
storage overhead at zero while structured objects remain typed and
versioned. Determinism is preserved end to end: tree hashes are
canonical regardless of insertion order, so structurally identical
worlds hash identically across runs and processes.

Error handling follows the fail-loud principle. Missing objects, corrupt
serialization, invalid branch names (rejected before touching the
filesystem to prevent path traversal), and attempts to delete the
current branch all produce errors; garbage collection aborts rather than
silently dropping data it cannot parse.

The CLI's `commit` reads chunk files dropped into `.chunklog/staging/`
(files named `<x>,<z>`), commits them, and clears the directory. This
mirrors Git's index in miniature and gives the tool a complete
end-to-end workflow without requiring a game binary.

## 5. Evaluation

We evaluate correctness across storage backends and then measure the
performance claims of Section 3: incremental commits, constant-time
checkout, and the comparison against naive full copies. All benchmarks
use 256-byte synthetic chunk payloads (a deliberately small chunk to
stress per-object overhead) and run under Criterion 0.5 on a desktop
machine (Windows, release profile).

### 5.1 Correctness

The test suite comprises 33 tests: 32 integration tests plus one
doctest. It covers object serialization round-trips and hash
determinism; store read/write/list/delete and deduplication; the full
commit/load round-trip; branch create/switch/delete semantics including
detached HEAD and unborn branches; diff correctness for
added/modified/removed chunks; and garbage collection, including the
property that objects reachable through *any* branch (not just HEAD) are
retained, and that unreachable objects are reclaimed.

Crucially, the entire suite is written against the `ObjectStore` trait,
and a significant subset runs twice: once with `FilesystemStore` and
once with an in-memory store (`MemoryStore`, ~30 lines). This cross-
backend validation is a direct consequence of the storage abstraction,
and it exercises the same code paths games would use with a custom
backend.

A notable defect discovered during development illustrates the value of
the tests: an early garbage-collection implementation attempted to parse
every reachable hash — including blob hashes — as a serialized object.
Because blobs are raw bytes, this failed on valid data. The failure
modes were caught by the memory-store tests before any filesystem
data could be lost, and the algorithm was restructured into strict
two-phase traversal (commits, then trees) with blobs marked reachable by
reference.

### 5.2 Performance

Table 1 summarizes Criterion measurements at world sizes of 100, 1,000,
and 10,000 chunks.

| Operation | 100 chunks | 1,000 chunks | 10,000 chunks |
| --- | ---: | ---: | ---: |
| commit | 7.7 ms | 38.5 ms | 313.8 ms |
| load (eager, full world) | 3.4 ms | 21.3 ms | 200.4 ms |
| checkout (reference move) | 1.2 ms | 1.3 ms | 1.9 ms |
| naive full copy (baseline) | 14.4 ms | 254.9 ms | 1,956 ms |

*Table 1. Median wall-clock time per operation. Chunk payload 256 bytes.
Baseline: writing every chunk to its own file.*

Three results stand out.

**Checkout is constant-time.** Median checkout time grows from 1.2 ms to
1.9 ms as the world grows a hundredfold, remaining within the noise of
process overhead. This validates the reference-only switching design:
the cost is independent of world size because no data is touched.

**Commit and load scale with the world, not the history.** Commit
throughput is roughly 32K chunks/s and load roughly 50K chunks/s,
determined by per-object hashing and file I/O. These numbers bound the
absolute cost of a save; the incremental property (Section 3.3) means
the *amortized* cost of a typical save is a fraction of these figures:
a save that edits 2 of 10,000 chunks performs two blob writes plus one
tree and one commit object.

**chunklog dominates naive full copies.** At 10,000 chunks, a commit
costs 314 ms against 1,956 ms for copying every chunk to disk — a 6.2x
reduction in save time, with storage savings in addition (Section 5.3).

### 5.3 Deduplication Effectiveness

We quantify deduplication with the integration example: a game world of
16 chunks, where a second save edits 2 chunks. The repository holds 22
objects (2 commits, 2 trees, 18 blobs: 16 original + 2 edited). A naive
full-copy scheme would store 32 chunk payloads. The saving grows
linearly with save count and edit sparsity: after *n* saves editing *k*
chunks each, a naive scheme stores *O(n·N)* payloads, while chunklog
stores *O(N + n·k)* — history cost is proportional to the total number
of edits, not to the number of saves. This is the precise sense in
which chunklog provides "incremental saving."

### 5.4 Storage Model Analysis

A simple model makes the scaling concrete. Let *N* be the world size in
chunks, *k* the expected edited chunks per save, *s* the payload size,
and *n* the number of saves. Naive full copies store *n·N·s* bytes and
write *n·N·s* bytes. chunklog stores roughly *N·s + n·k·s* bytes and
writes *O(k·s)* per save after the first. For a world of 10,000 chunks
saving every 5 minutes (288 saves per day) with 1% of chunks edited,
naive storage after a day is ≈288× the world size, while chunklog's is
≈3.9× — a 74x reduction — and per-save I/O is reduced by a factor of
100.

## 6. Discussion and Future Work

**Limitations.** Deduplication operates at chunk granularity: editing a
single block rewrites the chunk's entire payload, since the compressed
bytes change wholesale. Git enjoys finer-grained dedup because text
lines survive edits; voxel chunk data, especially compressed, does not.
The resulting storage bound (*O(N + n·k)* payloads) is strong but not
the *O(N + Δ)* that delta compression at sub-chunk level would provide.
The hash choice (SHA-256) resolves collision concerns at the cost of
throughput relative to non-cryptographic alternatives; at the measured
commit rates the difference is not yet visible in practice. Finally,
validation to date is synthetic: the shipped integration example is a
headless simulation, not a real engine. Both Minetest and Veloren expose
chunk serialization hooks that would make a production integration a
direct next step.

**Future work.** Three threads follow naturally. (1) *Delta
compression*: storing per-chunk deltas against a base version would
reduce the *n·k* term further for high-edit-frequency worlds. (2)
*Distributed workflows*: remote synchronization (push/pull), merge, and
cherry-pick across branches would make chunklog usable for multiplayer
server operations and cross-machine world sharing; the storage
abstraction already anticipates network backends. (3) *Operational
policies*: automatic pruning (gc on a schedule) and integrity
verification (recomputing hashes on read) would close the loop on the
operational story for long-lived servers.

## 7. Conclusion

We presented chunklog, a version-control library and CLI for voxel
worlds built on Git's content-addressed object model. Worlds are maps
of coordinates to opaque chunk bytes; blobs, trees, and commits form an
immutable object graph addressed by SHA-256, making deduplication
inherent and commits incremental. Checkout is a reference move —
constant-time at any world size — while world materialization is
explicitly deferred to the consumer. A byte-level storage abstraction
keeps the library engine- and backend-agnostic, and the entire system is
validated by a storage-independent test suite and a benchmark suite that
demonstrates a 6.2x save-time improvement over naive copies at 10,000
chunks with storage that grows with edits rather than saves. The
architecture generalizes to any domain with large, frequently mutated,
binary-treed content; voxel worlds are simply the clearest instance.

## References

[1] S. Chacon and B. Straub, *Pro Git*, 2nd ed. New York, NY, USA:
Apress, 2014.

[2] J. Benet, "IPFS — content addressed, versioned, P2P file system,"
arXiv preprint arXiv:1407.3561, 2014.

[3] L. Potter, "casync — content-addressable data synchronizer,"
2017. [Online]. Available: https://github.com/systemd/casync

[4] C. Walters, "OSTree: operating system and container deployment and
upgrades," 2016. [Online]. Available: https://ostree.readthedocs.io

[5] Mojang Studios, "Anvil file format," Minecraft Wiki. [Online].
Available: https://minecraft.wiki/w/Anvil_file_format

[6] Minetest Team, "Minetest engine documentation — world format."
[Online]. Available: https://docs.minetest.net/developers/world_format

[7] Veloren contributors, "Veloren — an open-source voxel RPG."
[Online]. Available: https://veloren.net

[8] National Institute of Standards and Technology, "Secure hash
standard (SHS)," FIPS PUB 180-4, 2015.
