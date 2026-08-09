# chunklog repository format 2

This document defines the durable format used by `chunklog` 0.3.0. Integer fields in canonical objects are big-endian. Hashes are 32-byte BLAKE3 digests of the complete canonical object bytes.

## Repository layout

```text
.chunklog/
├── FORMAT          # `2\n`
├── HEAD            # `ref: refs/heads/<name>` or a 64-hex commit address
├── LOCK            # present only while a writer is active
├── objects.sqlite3 # default SQLite content-addressed object store
├── refs/heads/     # validated branch-name files containing commit addresses
└── staging/        # incremental CLI patch
```

Repositories without `FORMAT`, the former experimental postcard layout, and format-1 loose repositories are rejected rather than guessed or silently mixed with format 2. The optional `FilesystemStore` keeps objects as `.chunklog/objects/<hex-hash>` files and is opened explicitly through `init_loose`/`open_loose`; it uses the same format-2 canonical object model but different physical storage.

## SQLite object store

The default store is a SQLite database containing:

```sql
CREATE TABLE objects (
    hash BLOB PRIMARY KEY NOT NULL CHECK(length(hash) = 32),
    data BLOB NOT NULL
) WITHOUT ROWID;
```

It uses rollback journaling, `synchronous=FULL`, a 30-second busy timeout, and one application writer guarded by `.chunklog/LOCK`. `hash` is the raw 32-byte BLAKE3 address. Every returned `data` value is rehashed before use.

All objects for one repository commit are inserted under `BEGIN IMMEDIATE` and made visible with one `COMMIT`. The object transaction commits before HEAD or a branch ref is replaced. Therefore:

1. failure before the SQLite commit rolls back all new object rows and leaves the ref unchanged;
2. failure after the SQLite commit but before ref replacement may leave unreachable objects, which GC can reclaim;
3. a ref is never deliberately published before its referenced objects commit.

The SQLite database transaction does not include the filesystem ref replacement, so the overall repository commit is not a cross-file transaction.

## Canonical object envelope

The repository marker is format 2, while the canonical object envelope remains wire version 1:

```text
43 48 4c 47       # ASCII `CHLG`
01                # canonical object version
tag               # object type
```

Tags provide domain separation:

| Tag | Type | Body |
| ---: | --- | --- |
| 1 | Blob | `u64 payload_length`, payload bytes |
| 2 | Tree branch | `u16 child_count`, then sorted (`u8 nibble`, 32-byte child hash) entries |
| 3 | Tree leaf | `i32 x`, `i32 z`, 32-byte Blob hash |
| 4 | Commit | 32-byte Tree root, parent flag and optional hash, `u64` Unix timestamp, `u64` UTF-8 message length, message bytes |

Decoders reject unknown versions/tags, invalid branch nibbles, duplicate nibbles, truncated fields, invalid UTF-8 messages and trailing bytes.

## Coordinate radix tree

A coordinate `(x, z)` is encoded as the concatenation of the big-endian two's-complement bytes of `x` and `z`. The eight bytes are traversed as sixteen high-then-low nibbles. A leaf exists at depth 16 and repeats the exact coordinate so readers can verify that it appears under the correct path.

Branches are sparse `nibble → child hash` maps. Empty worlds use one empty branch as the root. Empty non-root branches are removed. A change copies only the union of affected root-to-leaf paths; untouched children retain their hashes.

## Integrity

On every object read, the implementation hashes the complete canonical bytes and verifies that the digest equals the requested address. Existing objects are verified before an idempotent write succeeds. Repository code repeats this check for custom stores.

## References and writers

Branch names are validated before path construction and must be a single non-hidden path component without whitespace or `..`. Resolved paths must have `refs/heads` as their direct parent.

HEAD and branch files are written through a synced temporary file and platform atomic replacement. Mutating operations create `.chunklog/LOCK` with `create_new`; concurrent writers fail. A process crash can leave a stale lock, which must only be removed after an operator confirms that no writer is active.

## Garbage collection

GC holds the writer lock, snapshots HEAD and all branch roots, verifies and marks every reachable Commit, Tree node and Blob, and only then sweeps unmarked addresses. A marking error performs no deletion. On `SqliteStore`, all deletes occur in one SQLite transaction and a failure rolls them back. The generic `ObjectStore` contract does not require transactional deletion; loose/custom backends remain retryable but may retain a partially completed sweep.
