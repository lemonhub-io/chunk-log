# chunklog OPFS object-log format 1

This document defines the physical format used by the wasm-only `OpfsStore`.
It is an object-store format, not the native `.chunklog` repository format in
`FORMAT.md`.

## Execution model

`OpfsStore::open` obtains the origin-private root, creates or opens one file,
and acquires a `FileSystemSyncAccessHandle`. Browser callers must therefore
construct and use the store in a dedicated worker. A store instance owns its
access handle until drop.

## File and record framing

The file begins with the eight-byte header:

```text
43 48 4f 50 01 00 00 00   # `CHOP`, version 1, reserved zeros
```

Each following record has this envelope:

```text
kind:       u8
length:     u64 big-endian
payload:    `length` bytes
```

Record kinds and payloads are:

| Kind | Name | Payload |
| ---: | --- | --- |
| 1 | BEGIN | `u64 transaction_id` |
| 2 | PUT | 32-byte BLAKE3 hash, object bytes |
| 3 | DELETE | 32-byte BLAKE3 hash |
| 4 | COMMIT | `u64 transaction_id` |

PUT data is addressed by the BLAKE3 digest of the complete bytes supplied to
`ObjectStore::write`. Parsing and reads verify this digest. Unknown kinds,
malformed lengths, nested transactions, mismatched transaction identifiers,
and operations outside a transaction are corruption errors.

## Commit and recovery

A batch stages changes in wasm memory. Commit serializes BEGIN, the final
PUT/DELETE view, and COMMIT into one contiguous buffer, writes that buffer with
one synchronous access-handle call, then calls `flush`. In-memory changes are
visible to the owning store during a batch, but the durable index is published
only after the flush succeeds. Reopening applies only transactions that have a
valid COMMIT.

On open, the implementation scans from the file header and remembers the end
of the last committed transaction. A truncated record or an unterminated final
transaction is treated as a crash tail and physically truncated. Structural
corruption before that tail is rejected instead of guessed or skipped.

Automatic single-operation writes and deletes use the same transaction
protocol. An explicit batch should be used for initial imports to amortize the
durable flush.

## Space reclamation and limits

DELETE removes an address from the live index but does not rewrite earlier
records. There is no compaction API in format 1. Opening is linear in log size
and reads, verifies and retains the complete log in wasm memory as a coherent
read cache. Commit temporarily holds pending payloads and their serialized
transaction in addition to the retained log. Very large imports therefore need
bounded batches or a future checkpoint/segmentation design. Offsets are rejected
above JavaScript's exact integer range (`2^53 - 1`), and browser quota, eviction
and origin-clearing policy remain authoritative.
