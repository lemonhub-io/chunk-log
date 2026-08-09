# chunklog repository format 1

This document defines the durable format used by the current implementation. Integer fields are big-endian. Hashes are 32-byte BLAKE3 digests of the complete canonical object bytes.

## Repository layout

```text
.chunklog/
├── FORMAT          # `1\n`
├── HEAD            # `ref: refs/heads/<name>` or a 64-hex commit address
├── LOCK            # present only while a writer is active
├── objects/        # files named by lowercase 64-hex address
├── refs/heads/     # validated branch-name files containing commit addresses
└── staging/        # incremental CLI patch
```

Repositories without `FORMAT`, including the former experimental postcard format, are rejected rather than guessed or silently mixed with format 1.

## Canonical object envelope

Every object starts with:

```text
43 48 4c 47       # ASCII `CHLG`
01                # object format version
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

On every object read, the implementation hashes the full bytes and verifies that the digest equals the requested filename/address. Existing objects are verified before an idempotent write succeeds. Repository code repeats this check for custom stores.

## References and writers

Branch names are validated before path construction and must be a single non-hidden path component without whitespace or `..`. Resolved paths must have `refs/heads` as their direct parent.

HEAD and branch files are written through a synced temporary file and platform atomic replacement. Mutating operations create `.chunklog/LOCK` with `create_new`; concurrent writers fail. A process crash can leave a stale lock, which must only be removed after an operator confirms that no writer is active.

Object publication and reference publication are ordered, so ordinary process interruption before a reference update can leave unreachable immutable objects but cannot make a reference point to a partially published object. The default file-per-object backend does not issue a durability barrier for every object and therefore does not claim sudden-power-loss durability. The overall commit is not a cross-file transaction.

## Garbage collection

GC holds the writer lock, snapshots HEAD and all branch roots, verifies and marks every reachable Commit, Tree node and Blob, and only then sweeps unmarked addresses. A marking error performs no deletion. A delete failure can leave a prefix of unreachable objects deleted; GC is idempotent and safe to retry, but sweep is not crash-atomic.
