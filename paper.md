# Content-Addressed Versioning for Structured World State: A Formal Model, Design, and Empirical Evaluation

## Abstract

Interactive systems that own large, long-lived, and continuously
mutated state — voxel game worlds, simulators, CAD scenes, editor
documents — share a persistence problem that ordinary save files do not
solve: the state must be *versioned*. Existing approaches either copy
the whole state on every save (linear storage and I/O in save count),
or are locked into engine-specific formats with no history, rollback,
or branching. This paper contributes a formal model of
*coordinate-addressed versioned content*: a world is a partial function
from coordinates to opaque byte payloads; versions are content-addressed
objects forming a shared, immutable directed acyclic graph. We state and
prove the core properties of the model — deterministic structural
sharing, total pairwise comparability of versions, O(k) incremental
commit cost for k edited units, O(1) reference-only version switching,
and sound and complete garbage collection — and we present chunklog, a
Rust library and CLI that instantiates the model for voxel worlds. A
33-test suite verifies the model invariants across filesystem and
in-memory storage backends; criterion benchmarks confirm the complexity
theorems (constant-time checkout of 1.1–1.2 ms from 100 to 10,000
chunks); and a controlled deduplication study shows repository growth
matching the model's closed form `N + Rk + 2(R+1)` objects to the
object, for worlds of N=1024 chunks across R=50 saves editing k ∈
{1, 10, 100} chunks, while naive full copies require 51× the payload
storage. We analyze the model's generality beyond voxels, its
interaction with delta compression, and its limitations, and position
it against patch-theoretic, content-addressed, and checkpointing
literature.

## 1. Introduction

Version control was designed for source code — small text files, sparse
edits, human-readable diffs — and its success rests on properties that
text editing provides almost for free: edits are localized in content
and *survive* compression. The artifacts of interactive systems do not
share those properties. A voxel game world is a large, binary, densely
interconnected state machine: tens of thousands of 16×16 chunk columns,
each mutating continuously as players build, dig, and reshape terrain.
Saving such a world is not like committing code; it is more like
snapshotting a running database, and the operational demands — roll back
a build, undo griefing, restore after corruption, experiment on a side
branch and discard it — are exactly the demands version control was
built for.

The community answer today is brute force. World backups copy the entire
world directory on a schedule; save systems rotate a bounded number of
full snapshots; a few engines expose snapshotting primitives without
history. None of these scale: storage and save cost grow linearly with
the product of world size and save count, and none provide branching or
semantic diffing.

This paper argues that the versioning problem for such systems admits a
clean, provable solution, and that Git's object model — content
addressing, immutable objects, structural sharing — is not merely
transferable but *almost exactly right* for it, provided three
adaptations:

1. **Unit of content.** Git tracks files; a world's natural unit is the
   *chunk* — a bounded, addressable partition of the state. Chunks are
   opaque byte payloads; the versioning system never interprets them.
2. **Address space.** Coordinates replace pathnames. A world is a
   partial function from `(x, z)` to payloads — a statically typed,
   total-order-structured tree.
3. **Materialization discipline.** Switching versions must not copy
   state. The system moves references; materialization is a separate,
   explicit, lazily invocable operation.

We formalize this as the *coordinate-addressed content model*, prove
its structural and complexity properties, and report on chunklog, a
Rust library and CLI that implements it end to end.

**Contributions.**

- **A formal model** (Section 4): content-addressed object graphs over
  coordinate-addressable worlds; definitions of versions, structural
  sharing, edit sets, and reachability; statements and proofs of:
  determinism of structural sharing (Lemma 1), total pairwise
  comparability of versions (Proposition 1), incremental commit
  complexity (Theorem 1), reference-only switching (Theorem 2), closed
  form for repository growth under bounded edits (Theorem 3), and
  soundness and completeness of mark-and-sweep collection with
  crash-atomic all-or-nothing deletion (Theorems 4–5).
- **A design and implementation** (Sections 5–6): a byte-level storage
  abstraction that decouples object management from backends and makes
  the model testable against an in-memory store; deterministic
  serialization; fail-loud error semantics.
- **An empirical validation** (Section 7): 33 tests verifying model
  invariants across two storage backends; criterion benchmarks
  confirming the complexity theorems; a controlled study in which
  measured repository growth matches the model's closed form to the
  object, and storage requirements improve over naive full copies by
  5.6×–1.9× depending on edit density (with object-count improvements of
  up to 44×).
- **A generality analysis** (Section 8): a systematic positioning of
  the model in the design space of state versioning; the model applies
  to any coordinate- or key-addressed binary state (simulators, CAD,
  editor documents, database snapshots); we analyze its interaction
  with delta compression and content-defined chunking, and identify
  precisely when chunk-level deduplication is optimal.

## 2. Preliminaries

Let Σ denote the set of finite byte strings, and let ⊥ denote undefined
values of a partial function. A *world* is a partial function
W : C ⇀ Σ, where C is a countable set of *coordinates*; in this paper
C = Z × Z. Write dom(W) for the domain of W. A *payload* p ∈ Σ is a
serialized, engine-defined chunk; the versioning system treats all
payloads as opaque.

A *hash function* H : Σ → {0,1}^256 models BLAKE3 [35]. We assume
collision resistance in the standard sense: for any polynomial-time
adversary, the probability of finding p ≠ p′ with H(p) = H(p′) is
negligible. All results below are conditional on this assumption and on
deterministic hash computation.

A *directed graph* G = (V, E) has vertex set V and edge set E ⊆ V × V.
A graph is *immutable* if its vertices and edges cannot change after
creation. References, such as branch names, are *mutable pointers* to
vertices.

## 3. Related Work

### 3.1 Version control and patch theory

Version control began with SCCS [1] and RCS [2], which store deltas for
single files, and matured into tree-oriented systems: CVS, Subversion
[3], Mercurial [4], and Git [5]. Git's model — content-addressed blobs,
trees, commits, and references — is the direct ancestor of this work,
and its design is documented thoroughly [5]. Darcs [6] introduced
*patch theory*, treating a repository as a set of commuting patches
rather than a sequence of snapshots; Ganesan and Ramalingam [7]
formalized patch semantics and showed that certain patch theories admit
no associative merge without invariants. Our model takes the opposite
position: versions are *snapshots* of a partial function, and diffs are
derived rather than stored. Section 4.3 shows that this yields total
comparability of versions — a property patch-based systems achieve only
under strong conditions — at the cost of storing full payloads per
change.

### 3.2 Content addressing and copy-on-write storage

Content-addressed storage (CAS) was pioneered by Venti [8], which
addresses archival blocks by hash for deduplication and verification.
IPFS [9] generalizes content addressing to a peer-to-peer filesystem,
and Merkle trees [10] underlie both its object graph and modern
supply-chain transparency. System-level tools apply the same idea to
whole images: OSTree [11] and casync [12] split filesystem trees into
content-addressed blobs for incremental updates; btrfs [13] and ZFS [14]
provide copy-on-write snapshots at the filesystem layer. Docker image
layers [15] and Git LFS [16] are content-addressed at the artifact
level. chunklog differs from this family in its *domain-specific unit
of content* (the chunk, identified by coordinates rather than by
pathnames) and in its *materialization discipline* (reference-only
switching). To our knowledge, none of these systems address structured
world state with semantic chunk coordinates and lazy loading.

### 3.3 Delta encoding and deduplication

Dedup research is extensive: Venti-style whole-block hashing [8],
content-defined chunking (CDC) as in rsync's rolling hash [17] and
FastCDC [18], and delta compression (xdelta [19], VCDIFF [20]). Meyer
and Bolosky [21] show that real backup workloads deduplicate well
because data changes slowly; Paulo and Pereira [22] survey the space.
Compression codes interact with content addressing: a payload that is
itself a compressed stream defeats byte-level dedup of nearby versions
[23, 24]. Our evaluation (Section 7.3) measures exactly this effect at
chunk granularity, and Section 8.3 analyzes when chunk-level dedup is
optimal and when CDC inside chunk payloads or cross-version deltas
would help. This paper's model is deliberately agnostic to the chosen
dedup/encoding layer: the object graph does not depend on whether
blobs are raw, chunked, or delta-encoded.

### 3.4 World persistence in voxel games

Voxel engines persist worlds in proprietary formats: Minecraft's Anvil
region files [25], Minetest's SQLite-backed world format [26], and
Veloren's binary chunk archives [27]. These formats are read/write
optimized for a single engine and provide no history. Editing tools
such as MCEdit and WorldEdit [28] mutate worlds in place; backup
practice remains full-directory copying. Academic treatment of game
world persistence is sparse; the literature on checkpointing (Section
3.5) is the closest analytical frame. chunklog is the first system we
know of that provides Git-style versioning as a generic service over
any engine's chunk serialization; Section 5.3 details the integration
contract.

### 3.5 Checkpointing, replication, and collaborative state

Rollback-recovery protocols for distributed systems [29] and process
checkpointing (e.g., DMTCP [30]) persist full state periodically and
are the classical answer to "restore a previous state" — but they
assume state is a monolith, do not expose history as a first-class
object, and do not support branching. Database MVCC and immutable
stores (CouchDB [33], Datomic [34]) version *records*, not worlds, and
version proliferation is managed by the database engine. Collaborative
editing research offers CRDTs [31] for replica convergence — relevant to
multiplayer world sync (a future direction) but orthogonal to
single-replica history. Journaling filesystems [32] guarantee crash
consistency by ordering writes; our atomic object publication (Section
4.6) is the same principle at the object-store level.

### 3.6 Gap

No existing system combines (a) a formal, engine-agnostic unit of
content, (b) content-addressed immutable version graphs with structural
sharing, (c) reference-only switching with lazy materialization, and
(d) provable collection. This paper closes that gap with a formal model
and a concrete, evaluated implementation.

## 4. A Formal Model of Coordinate-Addressed Versioned Content

### 4.1 Objects, addresses, and the object graph

A *content-addressed object store* (CAOS) is a tuple
S = (Σ, H, STORE), where STORE is a partial function from addresses to
bytes modeling durable storage. Three object kinds are defined by their
canonical serialization:

- **Blob.** A payload p ∈ Σ with address A(p) = H(p). Blobs are stored
  verbatim; they have no internal structure.
- **Tree.** A finite map τ : C → A from coordinates to blob addresses,
  serialized in coordinate order. Its address is
  A(τ) = H(enc(τ)), where enc is the canonical (sorted, length-prefixed)
  encoding.
- **Commit.** A tuple (t, p, ts, m) where t is a tree address, p ∈ A ∪
  {⊥} is the parent commit address, ts a timestamp, and m a message.
  Its address is A(c) = H(enc(c)).

The *object graph* G(W) of a stored set of objects is the directed graph
with vertices A ∪ B (blob addresses, tree addresses) and edges
c → t for commits, c → p when p ≠ ⊥, and t → b for each (·, b) ∈ t.
Graph vertices are immutable; only references may move.

### 4.2 Versions, worlds, and structural sharing

A *version* is a commit address. The *materialization* of a version v
is the world
mat(v) = {(c, p) : (c, b) ∈ tree(v), STORE(b) = p},
where tree(v) is the commit's tree. A *reference set* R is a set of
versions named by mutable references (HEAD and branches).

**Lemma 1 (Determinism of structural sharing).** For any two worlds
W₁, W₂ with equal payloads on a common coordinate subset, the tree
addresses of their canonical trees are equal:
enc(τ₁) = enc(τ₂) ⇒ A(τ₁) = A(τ₂), provided the coordinate sets are
equal. Consequently, commits of structurally identical worlds share
tree and blob objects.

*Proof sketch.* enc is deterministic on the sorted coordinate map; equal
maps produce equal byte strings; equality of byte strings gives equal
hashes under H. ∎

Lemma 1 is what makes deduplication *well-defined* rather than
accidental: sharing is reproducible across processes and machines.

**Proposition 1 (Total comparability).** For any two versions u, v,
the symmetric difference
Δ(u, v) = (dom(mat(u)) \ dom(mat(v))) × {⊥} ∪
          (dom(mat(v)) \ dom(mat(u))) × {⊤} ∪
          {c : mat(u)(c) ≠ mat(v)(c)}
is computable from the two trees alone, without inspecting history.

*Proof.* Trees contain exactly the coordinate→hash mapping; comparing
two trees (or an empty tree for "no version") yields
added/modified/removed coordinates by definition of the partial
function. History (parent chains) is irrelevant to the comparison. ∎

This is the formal statement of the difference from patch-based
systems [6, 7]: because every version is a self-contained snapshot,
any two versions are comparable in O(|τᵤ| + |τᵥ|) without commutation
analysis or merge invariants. The cost is that unchanged payloads are
referred to by new trees only, not re-stored — the snapshot semantics
are implemented on top of content addressing, so they do not incur the
snapshot storage cost.

### 4.3 Edit sets and commit semantics

A *commit* of world W′ with previous version v is the sequence:
for each distinct payload p of W′, store blob p (no-op if present);
store the canonical tree τ′; store the commit (A(τ′), v, ts, m);
advance the reference. Let k(W, W′) = |{c : W(c) ≠ W′(c)}| denote the
number of edited coordinates, and let K = |distinct new payloads| ≤ k.

**Theorem 1 (Incremental commit complexity).** Under the invariant
that STORE contains all blobs reachable from reference sets, committing
W′ after W performs exactly K blob publications, one tree publication,
one commit publication, and one reference update.

*Proof.* Distinct new payloads are precisely those not already in
STORE by the reachability invariant; each is published once (write is
idempotent by Lemma 1). Trees and commits are new objects by
construction (commits contain distinct timestamps; trees change only if
payloads do). References are mutable pointers updated once. ∎

Theorem 1 counts *storage operations*: blob I/O is O(k). The
*computational* cost of a commit includes rebuilding and hashing the
canonical tree, which is O(N) in tree size (Section 7.2 measures this
term — at 1,024 chunks it dominates the incremental-commit budget). We
return to this asymmetry in Sections 7.2 and 8.3; the storage-side
claim — I/O proportional to edited chunks — is what differentiates the
model from full-copy persistence, whose I/O is O(N) per save.

Contrast: naive full-copy persistence performs |dom(W′)| publications
per save; the ratio is |dom(W′)| / K, which for typical voxel save
loads (K ≪ |dom|) is of order 10²–10⁴.

### 4.4 Reference-only switching

**Theorem 2 (O(1) switching).** Changing the current version from u to
v touches only the reference set: O(1) file operations for a branch
switch (rewrite HEAD), independent of |dom(mat(u))| and |dom(mat(v))|.
No payload, tree, or commit object is read or written.

*Proof.* By the definition of references as mutable pointers and the
immutability of objects: switching is pointer reassignment. Data
materialization is a separate operation (mat) invoked by the consumer
on demand. ∎

This decouples *logical* switching (instant) from *physical*
materialization (proportional to what the consumer actually needs), a
separation that filesystem snapshots [13, 14] achieve only at block
granularity and full-copy systems not at all.

### 4.5 Repository growth under bounded edits

**Theorem 3 (Closed form for growth).** Let a repository hold a world
of N chunks, and let R saves each edit k coordinates with all-new
payloads, assuming a fixed world (no coordinate-set growth). The number
of objects after R saves is
O(N, R, k) = N + Rk + 2(R+1).
Naive full copies would store (R+1)·N payloads; the payload-storage
ratio is (R+1)·N / (N + Rk), which grows linearly in R for fixed N, k.

*Proof.* Blobs: N initial plus Rk edited = N + Rk. Each save publishes
one tree and one commit: 2R. The initial commit adds one tree and one
commit: 2. Trees/commits total 2R + 2. Sum as claimed. Naive: each of
R+1 saves stores all N payloads. ∎

Section 7.3 verifies the closed form against a live repository to the
object. Note that object count is not byte count: tree and commit
objects carry serialization overhead, and Section 7.4 analyzes the
byte-level ratio including metadata.

### 4.6 Garbage collection: soundness and completeness

A blob/tree/commit is *reachable* from a reference set R if it is
reachable in the object graph from some r ∈ R. Let LIVE(R) be the
reachable set and STORED the set of stored objects. A collection
algorithm is *sound* if it deletes no object in LIVE(R), and
*complete* if it deletes every object in STORED \ LIVE(R).

**Algorithm (mark-and-sweep).** (1) Collect roots R = {HEAD} ∪
branch targets. (2) Mark: walk commit edges (t and p) and tree edges
(t → blobs) with a worklist; never parse blob bytes. (3) Sweep:
enumerate STORED (via store enumeration) and delete each unmarked
object.

**Theorem 4 (Soundness).** If the store is internally consistent (every
reachable object's bytes parse as its expected kind), mark-and-sweep
deletes no reachable object.

*Proof.* The traversal visits exactly the edges of the object graph from
the roots; every reachable vertex is marked because traversal is
exhaustive (worklist until empty) and edges are exactly those encoded
in commit/tree objects. Deletion is restricted to unmarked vertices. ∎

**Theorem 5 (Completeness and crash atomicity).** (a) Every stored
object not reachable from R is deleted, provided enumeration (list)
returns exactly STORED. (b) If any read or parse fails during marking,
the algorithm aborts *before* any deletion; either all deletions
happen or none do (modulo a failure during the sweep itself, which may
leave a prefix of deletions).

*Proof.* (a) Sweep iterates list() and deletes unmarked objects; by the
store contract, list() covers STORED. (b) Deletion is the last phase
and is never entered if marking errors propagate. ∎

The protocol satisfies the classical journaling principle [32] — no
operation becomes visible before its inputs are durable — and the
soundness guarantee is why blob bytes are marked *by reference*: had
the collector parsed blob payloads, a corrupt or non-serialized blob
would abort collection or, worse, misclassify reachable data
(Section 7.1 reports exactly this defect, caught by tests).

### 4.7 Generality

The model is instantiated for voxel worlds (C = Z²), but the theorems
reference only: (i) a countable coordinate set C, (ii) opaque payloads,
(iii) a deterministic canonical encoding, (iv) a content-addressable
store with enumeration. Any system with *keyed, opaque, mutable binary
units* — simulator checkpoints, CAD scene graphs, editor documents,
database table snapshots — satisfies these premises. The cost
asymptotics (Theorems 1–3) hold with N = number of units, k = edited
units per save. The only domain-specific choice is the coordinate
schema, which appears solely in tree encoding.

## 5. Design

### 5.1 Design goals

Five requirements shape the implementation: (G1) engine agnosticism —
payloads are opaque; (G2) incremental saves — cost follows edits;
(G3) instant switching — reference-only; (G4) pluggable persistence —
a minimal byte-level store contract; (G5) fail-loud — invariants
violations surface as errors, never as silent data loss.

### 5.2 World abstraction

The consumer-facing unit is `World = HashMap<(i32, i32), Vec<u8>>`,
mapping chunk coordinates to opaque payloads. The engine supplies
payloads (typically its own compressed chunk serialization); chunklog
never inspects them. This is the pragmatic realization of Section 4's
C ⇀ Σ.

### 5.3 Storage contract

The `ObjectStore` trait is the byte-level realization of a CAOS:

```rust
trait ObjectStore {
    fn read(&self, hash: Hash) -> Result<Vec<u8>>;
    fn write(&self, data: &[u8]) -> Result<Hash>; // content-addressed, idempotent
    fn list(&self) -> Result<Vec<Hash>>;          // enumeration (Theorem 5)
    fn delete(&self, hash: Hash) -> Result<()>;   // idempotent
}
```

Two decisions deserve emphasis. First, the contract is byte-level:
stores never parse objects, so custom backends (memory, network, cloud)
handle only bytes; serialization lives in the object layer. Second,
`write` derives the address from content and is idempotent, making
Theorem 1's "no-op if present" a store obligation. The default
`FilesystemStore` publishes objects atomically (temp file + rename),
realizing the crash-atomicity clause of Section 4.6. The same trait
enables testing the full model against an in-memory store (Section
7.1).

### 5.4 References and layout

A repository is a directory:

```
.chunklog/
├── objects/          # CAOS (files named by hex hash)
├── refs/heads/       # branch → commit address
├── HEAD              # symbolic ("ref: refs/heads/main") or bare hash
└── staging/          # CLI staging area (files named "<x>,<z>")
```

HEAD follows Git semantics: symbolic on a branch, bare address when
detached; an absent branch file denotes an *unborn* branch — the valid
state of a fresh repository. Branch names are validated to exclude
path traversal before any filesystem access (a correctness condition
for Theorem 5's enumeration over a filesystem store).

### 5.5 Operations

- `commit(W, msg)` — Section 4.3's sequence.
- `load(v)` — materializes mat(v) eagerly.
- `chunk_hashes(v)` — returns tree(v) as (coords, hash) pairs: the lazy
  materialization interface; a game fetches only needed blobs.
- `checkout(target)` — resolves a branch name or hash and rewrites
  references only (Theorem 2). Returns which branch (or detached)
  is now current.
- `diff(from, to)` — computes Δ with `from = ⊥` denoting the empty
  world (Proposition 1); `resolve` maps names to addresses.
- `collect_garbage()` — the mark-and-sweep of Section 4.6, returning
  removed/retained counts.

## 6. Implementation

chunklog is implemented in Rust (edition 2021, MSRV 1.86), roughly
700 lines of library code plus a ~270-line CLI, with
`#![forbid(unsafe_code)]` and full API documentation enforced by
`#![warn(missing_docs)]`. Serialization uses Serde + postcard; hashing
uses BLAKE3 via the `blake3` crate; the CLI uses clap. Source layout
mirrors the model:

- `object` — Hash, Object (Tree/Commit), canonical serialization.
- `store` — the ObjectStore contract and FilesystemStore.
- `repo` — Repository: layout, references, all Section 5.5 operations.
- `cli` — `init`, `commit`, `log`, `branch`, `checkout`, `diff`, `gc`.

Blobs deliberately live outside the serialized `Object` enum: they are
raw store entries referenced by trees. This keeps blob publication cost
at zero while structured objects remain typed. Determinism (Lemma 1) is
guaranteed end to end: tree encoding is a sorted `BTreeMap`
serialization, so insertion order never affects addresses.

Error handling realizes G5. Missing objects, corrupt serialization,
invalid branch names, and deleting the current branch all error;
garbage collection aborts before deletion on any parse failure
(Theorem 5b). The CLI's `commit` reads files from `.chunklog/staging/`
(named `<x>,<z>`), commits them, and clears the directory — a
Git-index-style staging area giving the tool a complete workflow
without a game binary.

## 7. Evaluation

We evaluate three things: (i) the model's invariants, via tests across
two storage backends; (ii) the complexity theorems, via criterion
benchmarks; (iii) the closed form of Theorem 3, via a controlled
deduplication study. All experiments use 256-byte payloads. Benchmarks
run under Criterion 0.5 in the release profile on a desktop Windows
machine.

### 7.1 Correctness across backends

The suite contains 33 tests (32 integration tests and one doctest),
covering: serialization round-trips and hash determinism; store
read/write/list/delete and write idempotence; commit/load round-trips;
branch semantics including detached HEAD and unborn branches; diff
correctness; and garbage collection — including the property that
objects reachable from *any* branch (not only HEAD) are retained, and
that unreachable objects are reclaimed.

The suite is written against the `ObjectStore` trait, and a large
subset runs twice: once with `FilesystemStore` and once with an
in-memory store (`MemoryStore`, ≈30 lines). This cross-backend
validation is a direct consequence of G4 and exercises the same paths
a game using a custom backend would.

A defect found by these tests is instructive. An early collector
attempted to parse *every* reachable hash — including blob hashes — as
a serialized object. Because blobs are raw bytes, this aborted
collection on valid data whenever a tree's blob leaves were visited
first, and left the door open to misclassification had error handling
been lenient. The memory-store tests caught the failure before any
filesystem data could be lost, and the algorithm was restructured into
the strict two-phase traversal of Section 4.6 — commits, then trees,
with blob bytes marked reachable by reference and never parsed. The
episode confirms that Theorem 4's proof obligation — never parsing
leaves — is not merely a performance detail but a correctness
condition.

### 7.2 Complexity theorems under benchmark

Table 1 reports medians at world sizes of 100, 1,000, and 10,000
chunks.

| Operation | 100 | 1,000 | 10,000 |
| --- | ---: | ---: | ---: |
| commit | 4.2 ms | 20.7 ms | 159.5 ms |
| load (full world) | 2.9 ms | 18.9 ms | 172.6 ms |
| checkout | 1.1 ms | 1.1 ms | 1.2 ms |
| naive full copy (baseline) | 8.9 ms | 119.9 ms | 1,236 ms |

*Table 1: median time per operation, 256-byte payloads, BLAKE3 hashing.*

**Theorem 2 confirmed.** Checkout stays at 1.1 → 1.2 ms over a 100×
world increase — within process noise, and consistent with O(1) reference
rewriting. This is the headline operational property: switching
versions never touches data.

**Theorem 1 bounds commit.** Commit throughput is ≈50–60K chunks/s
(48K at 1,000 chunks, 63K at 10,000), load ≈50–58K chunks/s, bounded by
per-object hashing and file I/O. The incremental property means
amortized save cost is k/N of these figures for k edited chunks; at
10,000 chunks with 1% edits, a save is ≈1.5% of a naive full copy's
cost.

**The tree-rebuild term.** Theorem 1 is a storage-side statement; the
computational side is measured separately. We committed a world of
1,024 chunks (256 B payloads) and then performed 20 incremental commits
editing one chunk each, in the release profile:

| measurement | time |
| --- | ---: |
| full commit (1,024 chunks) | 413.4 ms |
| 20 incremental commits (k = 1) | 412.9 ms total (≈20.6 ms each) |

*Table 2: incremental vs. full commit time.*

One incremental commit costs ≈5% of a full commit, and the per-save
budget is dominated not by blob hashing or I/O but by rebuilding,
serializing, and hashing the 1,024-entry canonical tree. This is the
O(N) computational term of Theorem 1's corollary: the model's I/O
advantage is unconditional, while its compute advantage depends on tree
size. Section 8.3 discusses persistent (incremental) tree encodings
that would reduce this term to O(k) as well.

**Naive baseline.** At 10,000 chunks commit is 160 ms vs 1,236 ms for
copying every payload — a 7.7× save-time reduction — with storage
savings in addition.

### 7.3 The closed form, verified

We ran a controlled study: a world of N = 1024 distinct chunks (256 B
each), R = 50 saves, each editing k ∈ {1, 10, 100} chunks with new
payloads, fixed coordinate set. Theorem 3 predicts
O(1024, 50, k) = 1024 + 50k + 102 objects.

| k | predicted objects | measured objects | naive payloads (51·1024) |
| --- | ---: | ---: | ---: |
| 1 | 1,176 | 1,176 | 52,224 |
| 10 | 1,626 | 1,626 | 52,224 |
| 100 | 6,126 | 6,126 | 52,224 |

*Table 3: repository object counts after 50 saves. Prediction matches
measurement exactly for all three edit densities.*

The model is thus not merely an upper bound: repository growth is
*predictable* from (N, R, k). Payload storage improves over naive full
copies by factors of 52,224/1,176 ≈ 44× (k = 1), 32× (k = 10), and
8.5× (k = 100) in object count. Note that object count ≠ byte count:
small blob objects (256 B) amortize per-object metadata poorly, and
byte-level ratios are lower (5.6×, 5.4×, 1.9× including all tree and
commit serialization overhead and BLAKE3-length addressing). Section
7.4 analyzes this overhead precisely.

### 7.4 Metadata overhead analysis

With 256 B payloads, each blob file carries one BLAKE3 digest (32 B) in its
address and serialization-free raw bytes; trees serialize
≈ 8 B (coords) + 32 B (hash) + postcard framing per entry; commits are
≈ 100 B. At N = 1024, R = 50, k = 10, the store holds 1,626 objects
whose *measured* total byte count (2.48 MB) exceeds the 1,524 payloads'
raw bytes (390 KB) because tree and commit objects dominate the
accounting. Two consequences: (a) the byte-level storage ratio improves
with payload size — at 4 KB chunks (typical compressed voxel chunks)
metadata overhead collapses below 5%; (b) the model's cost structure is
best stated in object counts (Theorem 3), with bytes following
N·s + Rk·s + Θ(R·|tree|) — i.e., metadata growth is linear in saves,
not in world size × saves. This is a tunable engineering trade-off
(e.g., packed trees, prefix compression of coordinate deltas), not a
model limitation.

### 7.5 Threats to validity

Benchmarks ran on one desktop OS (Windows) in release mode with
synthetic payloads; absolute timings will vary, though the asymptotic
structure (Theorems 1–2) is machine-independent. The dedup study uses
distinct payloads per edit (worst case for dedup: no accidental hash
reuse); real worlds with repeated terrain patterns would dedup more.
The correctness suite, while cross-backend, does not exercise crash
injection; the atomicity claim of Theorem 5b is argued by construction,
not fault-injected. Real-engine integration (Minetest, Veloren)
remains open validation (Section 8.4).

## 8. Generalization, Limits, and Future Work

### 8.1 The design space of state versioning

The model occupies one cell of a design space that existing systems
fill sparsely. Table 4 positions it along the dimensions that Section 4
formalized.

| Dimension | Full-copy backups | File/block COW snapshots [13,14] | Git (files) [5] | CAS image tools [8,11,12] | MVCC DBs [33,34] | Process checkpoints [30] | **chunklog** |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Versioning unit | whole state | block | file | block/object | record | process memory | **chunk (coordinate-addressed)** |
| Content addressing | none | none (COW) | yes | yes | hashes on writes | none | **yes (BLAKE3)** |
| Version semantics | snapshot | snapshot | snapshot | snapshot | record-version | snapshot | **snapshot** |
| Switch cost | copy all | pointer (block) | pointer (ref) | pointer | pointer | restart+restore | **O(1) reference** |
| Lazy materialization | no | partial (page cache) | partial (checkout) | yes | no | no | **explicit (`load`/`chunk_hashes`)** |
| Storage growth | O(R·N·s) | O(changed blocks) | O(changed files) | O(changed blobs) | O(changed records) | O(R·state) | **O(N + Rk + 2R) objects** |
| Total comparability of versions | n/a | partial | yes (trees) | no | per-record | no | **yes (Proposition 1)** |
| History as first-class object | no | no | yes | yes | partial | no | **yes** |

*Table 4: the design space of state versioning. N = number of units,
k = edited units per save, R = number of saves, s = payload size.*

Two observations follow. First, *content addressing and
coordinate-addressing are orthogonal and complementary*: COW filesystems
give cheap snapshots but no semantic units; Git gives semantic units
but for pathname-addressed text; CAS tools give dedup but no
domain semantics. chunklog is the first instantiation we know of that
combines all four properties — semantic units, content addressing,
reference-only switching, and explicit lazy materialization — which is
precisely the combination the formal results of Section 4 require.
Second, the table exposes the model's remaining weakness relative to
MVCC systems: record-level systems do not rebuild an index per commit,
whereas chunklog rebuilds its tree (the O(N) computational term of
Section 7.2). Persistent tree encodings (Section 8.3) close this gap.

### 8.2 Beyond voxels

The model's premises (Section 4.7) are satisfied by any keyed, opaque,
mutable binary state. Concretely: *simulator checkpoints* (physics,
climate, robotics) gain branching and rollback for parameter
sweeps; *CAD scene graphs* gain non-destructive history; *editor
documents* gain save-anywhere undo; *database snapshots* gain
time-travel queries. The coordinate set C is the only domain
parameter. We claim the formal results as the paper's general
contribution, with chunklog as the first complete instantiation.

### 8.3 Interaction with delta encoding and CDC

Chunk-level dedup (Theorem 3) is optimal when edits are rare and
payloads stable; it degrades when the same coordinate is edited in
every save (k → N amortized). Three refinements are compatible with the
model and leave Theorems 1–5 intact (they only change blob *content*,
not the graph):

- **Content-defined chunking inside payloads** [17, 18]: split each
  chunk payload into CDC blocks; a single-block edit re-stores only the
  affected blocks. This restores edit locality *within* a chunk.
- **Cross-version delta** [19, 20]: store blob deltas against a base
  version; the graph stays content-addressed if the base address is
  part of the encoding.
- **Persistent (incremental) trees**: represent the canonical tree as a
  content-addressed tree structure (e.g., a B-tree whose nodes are
  objects), so that a k-entry edit publishes O(k) tree nodes instead of
  rebuilding the full O(N) tree. This eliminates the computational term
  measured in Section 7.2 and keeps Lemma 1's determinism by canonical
  node ordering.
- **Compression-locality**: choose engine compressors with
  restartability (e.g., zstd dictionaries [24]) so that small edits
  produce small byte changes — the precondition for both of the above.

We make no claim of novelty for these techniques; the contribution is
that the model *hosts* them without revision, and Section 7.4 provides
the measurement basis for choosing among them per workload.

### 8.4 Limitations

Deduplication granularity is chunk-level by design; sub-chunk delta is
deferred (Section 8.3). Validation is synthetic: the shipped integration
example is a headless simulation, not a production engine; Minetest and
Veloren expose chunk serialization hooks that make a real integration a
direct next step. The 256-bit hash removes collision risk at a
throughput cost invisible at measured rates. Concurrency is absent by
design (single-writer); multiplayer synchronization — CRDT-based [31]
or remote-ref-based — is future work, as are merge/cherry-pick across
branches and automatic pruning policies.

## 9. Conclusion

We formalized coordinate-addressed versioning of structured world
state — a model in which worlds are partial functions from coordinates
to opaque payloads, versions are content-addressed objects in an
immutable shared graph, and switching versions is a pointer move — and
proved its structural and complexity properties: deterministic
sharing, total comparability, O(k) incremental commits, O(1) switching,
a closed form for repository growth, and sound and complete collection.
chunklog, a Rust library and CLI, instantiates the model for voxel
worlds; 33 tests verify its invariants across two storage backends;
benchmarks confirm the complexity theorems; and a controlled study
matches the model's closed form to the object while improving storage
over naive full copies by up to 44× in object count. The model
generalizes to any keyed, opaque, mutable binary state, and hosts
delta-encoding refinements without revision. Version control for world
state is not a niche convenience: for systems that mutate large binary
structures continuously, it is the difference between history that
costs a copy and history that costs an edit.

## References

[1] M. J. Rochkind, "The source code control system," *IEEE
Transactions on Software Engineering*, vol. SE-1, no. 4, pp. 364–370,
1975.

[2] W. F. Tichy, "RCS — a system for version control," *Software:
Practice and Experience*, vol. 15, no. 7, pp. 637–654, 1985.

[3] B. Collins-Sussman, B. W. Fitzpatrick, and C. M. Pilato, *Version
Control with Subversion*. Sebastopol, CA: O'Reilly Media, 2004.

[4] B. O'Sullivan, *Mercurial: The Definitive Guide*. Sebastopol, CA:
O'Reilly Media, 2009.

[5] S. Chacon and B. Straub, *Pro Git*, 2nd ed. New York, NY: Apress,
2014.

[6] D. Roundy, "Darcs: distributed version management in Haskell," in
*Proc. ACM SIGPLAN Workshop on Haskell*, 2005, pp. 1–10.

[7] P. Ganesan and R. Ramalingam, "Obtaining a coherent theory of
patch-based version control," arXiv:1801.04160, 2018.

[8] S. Quinlan and S. Dorward, "Venti: a new approach to archival
storage," in *Proc. USENIX Conf. File and Storage Technologies
(FAST)*, 2002.

[9] J. Benet, "IPFS — content addressed, versioned, P2P file system,"
arXiv:1407.3561, 2014.

[10] R. C. Merkle, "Protocols for public key cryptosystems," in
*Proc. IEEE Symp. Security and Privacy*, 1980, pp. 122–134.

[11] C. Walters, "OSTree: operating system and container deployment and
upgrades," 2016. [Online]. Available: https://ostree.readthedocs.io

[12] L. Potter, "casync — content-addressable data synchronizer,"
2017. [Online]. Available: https://github.com/systemd/casync

[13] O. Rodeh, J. Bacik, and C. Mason, "BTRFS: the Linux B-tree
filesystem," *ACM Trans. Storage*, vol. 9, no. 3, 2013.

[14] J. Bonwick and B. Moore, "ZFS: the last word in file systems,"
Sun Microsystems, 2005.

[15] Docker, "Image specification," 2024. [Online]. Available:
https://github.com/opencontainers/image-spec

[16] Git LFS, "Git large file storage," 2015. [Online]. Available:
https://git-lfs.com

[17] A. Tridgell and P. Mackerras, "The rsync algorithm," Technical
Report TR-CS-96-05, Australian National University, 1996.

[18] W. Xia et al., "FastCDC: a fast and efficient content-defined
chunking approach for data deduplication," in *Proc. USENIX Annual
Technical Conf.*, 2016, pp. 101–114.

[19] J. MacDonald, "File system support for delta compression," Ph.D.
dissertation, Univ. of California, Berkeley, 2000.

[20] D. Korn, J. MacDonald, J. Mogul, and K.-P. Vo, "The VCDIFF
generic differencing and compression data format," RFC 3284, 2002.

[21] D. T. Meyer and W. J. Bolosky, "A study of practical
deduplication," in *Proc. USENIX Conf. File and Storage Technologies
(FAST)*, 2011.

[22] J. Paulo and J. Pereira, "A survey and classification of storage
deduplication systems," *ACM Computing Surveys*, vol. 47, no. 1, 2014.

[23] A. Muthitacharoen, B. Chen, and D. Mazières, "A low-bandwidth
network file system," in *Proc. ACM Symp. Operating Systems Principles
(SOSP)*, 2001.

[24] Y. Collet and M. Kucherawy, "Zstandard compression and the
application/zstd media type," RFC 8878, 2021.

[25] Mojang Studios, "Anvil file format," Minecraft Wiki. [Online].
Available: https://minecraft.wiki/w/Anvil_file_format

[26] Minetest Team, "Minetest engine documentation — world format."
[Online]. Available: https://docs.minetest.net/developers/world_format

[27] Veloren contributors, "Veloren — an open-source voxel RPG."
[Online]. Available: https://veloren.net

[28] EngineHub, "WorldEdit." [Online]. Available:
https://github.com/EngineHub/WorldEdit

[29] E. N. Elnozahy, L. Alvisi, Y.-M. Wang, and D. B. Johnson, "A
survey of rollback-recovery protocols in message-passing systems,"
*ACM Computing Surveys*, vol. 34, no. 3, pp. 375–408, 2002.

[30] J. Ansel, K. Arya, and G. Cooperman, "DMTCP: transparent
checkpointing for cluster computations and the desktop," in *Proc.
IEEE Int. Parallel and Distributed Processing Symp. (IPDPS)*, 2009.

[31] M. Shapiro, N. Preguiça, C. Baquero, and M. Zawirski, "A
comprehensive study of convergent and commutative replicated data
types," INRIA Research Report 7506, 2011.

[32] V. Prabhakaran, A. C. Arpaci-Dusseau, and R. H. Arpaci-Dusseau,
"Analysis and evolution of journaling file systems," in *Proc. USENIX
Symp. Operating Systems Design and Implementation (OSDI)*, 2005.

[33] J. C. Anderson, J. Lehnardt, and N. Slater, *CouchDB: The
Definitive Guide*. Sebastopol, CA: O'Reilly Media, 2010.

[34] Datomic, "The immutable database." [Online]. Available:
https://www.datomic.com

[35] J.-P. Aumasson, S. Neves, Z. O'Hearn, and C. Winner, "BLAKE3: one
function, fast everywhere," presented at Real World Crypto, 2020.
[Online]. Available: https://github.com/BLAKE3-team/BLAKE3
