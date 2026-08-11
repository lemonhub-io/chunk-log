# OPFS object-store benchmark and optimization report

Date: 2026-08-11

Final raw samples: [`opfs-benchmark-raw.json`](opfs-benchmark-raw.json)

Before optimization: [`opfs-benchmark-before-coalescing.json`](opfs-benchmark-before-coalescing.json)

Write-coalescing stage: [`opfs-benchmark-after-write-coalescing.json`](opfs-benchmark-after-write-coalescing.json)

## Environment

- Microsoft Windows 11 Home Chinese, build 10.0.26200, 64-bit;
- Intel Core i9-13900H, 14 cores / 20 logical processors, 39.6 GiB RAM;
- headless Chromium 131.0.6778.33 through Playwright Core 1.49.1;
- Rust 1.97.1, `wasm32-unknown-unknown`, `wasm-bindgen` 0.2.127;
- browser-reported OPFS quota: 2,327,214,648 bytes;
- one Dedicated Worker and one `FileSystemSyncAccessHandle` at a time.

## Method

Each object is a deterministic and unique 256-byte payload. Dataset generation
is outside the timed intervals. Every trial creates a fresh OPFS file, opens an
`OpfsStore`, writes N objects, closes it, reopens it, verifies the recovered
object count, reads and byte-compares every object, records physical file size,
then removes the file. Each scenario has one excluded warm-up trial.

`import_ms` includes BLAKE3 hashing, append-log framing, synchronous writes and
durable `flush`. Batched trials wrap all N writes in one explicit batch;
unbatched trials use one automatic transaction and flush per object.
`reopen_ms` includes reading the complete log, parsing it, verifying every PUT
digest, retaining the log cache and rebuilding the index. Final `read_all_ms`
rehashes and copies every object from that wasm cache; it is deliberately not a
second OPFS disk-read measurement. Values are medians with the 25th–75th
percentile interval, not confidence intervals.

Before measurement, every browser run also checks pending-put visibility,
rollback, pending-delete visibility, delete cancellation, committed deletion
and close/reopen persistence. The final JSON records
`batch_semantics_verified: true` only after these checks pass.

## Final results

| Scenario | Samples | Import | Reopen | Verified cached full read | Physical bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| batched N=100 | 9 | 0.9 ms [0.7, 1.1] | 2.0 ms [1.8, 2.4] | 0.1 ms [0.1, 0.2] | 29,742 |
| unbatched N=100 | 9 | 46.3 ms [44.4, 47.2] | 1.5 ms [1.5, 1.8] | 0.1 ms [0.1, 0.2] | 33,108 |
| batched N=1,000 | 7 | 3.3 ms [2.9, 3.6] | 2.7 ms [2.5, 3.0] | 0.7 ms [0.7, 0.7] | 297,042 |
| unbatched N=1,000 | 7 | 415.4 ms [367.4, 573.4] | 3.0 ms [2.7, 3.8] | 0.6 ms [0.6, 1.0] | 331,008 |
| batched N=10,000 | 5 | 39.0 ms [35.8, 39.9] | 18.6 ms [16.5, 19.0] | 13.5 ms [11.9, 13.7] | 2,970,042 |

Within the final implementation, one explicit batch is about 51× faster than
automatic per-object transactions at N=100 and 126× faster at N=1,000. It also
avoids per-object BEGIN/COMMIT records: at N=1,000 the log is 297,042 bytes
instead of 331,008 bytes, a 10.3% physical reduction. This is framing reduction,
not compression.

The final batched N=10,000 result corresponds to approximately 256,000
objects/s or 65.6 MB/s of payload during import. The verified cached read is
about 741,000 objects/s or 189.6 MB/s. Reopening remains the operation that
actually reads and verifies the 2.97 MB OPFS file, with an 18.6 ms median.

## Before/after attribution

| Metric | N | Before | Write coalescing only | Final | Overall speedup |
| --- | ---: | ---: | ---: | ---: | ---: |
| batched import | 100 | 30.1 ms | 0.7 ms | 0.9 ms | 33× |
| batched import | 1,000 | 446.9 ms | 2.6 ms | 3.3 ms | 135× |
| batched import | 10,000 | 5,069.7 ms | 34.1 ms | 39.0 ms | 130× |
| verified full read | 1,000 | 267.2 ms | 205.2 ms | 0.7 ms | 382× |
| verified full read | 10,000 | 2,964.0 ms | 3,473.0 ms | 13.5 ms | 220× |

The first change stages a batch in wasm memory and emits one contiguous OPFS
write followed by one flush. It also removes the former full-index clone at
`begin_batch`. The second change retains the already scanned log as a coherent
read cache while the exclusive access handle is owned. These changes remove
the two dominant repeated Wasm/JavaScript boundary crossings without changing
the durable log format or physical file size.

## Memory and interpretation boundaries

- Coalescing trades I/O calls for memory: pending payloads and the serialized
  transaction coexist during commit. The store also retains the complete log
  for its lifetime. This is suitable for the measured sizes but still requires
  chunked transactions or checkpoints for very large logs.
- This is a single-host, single-browser, headless microbenchmark. It does not
  establish cross-browser, mobile or production-engine performance.
- It exercises `ObjectStore` with raw unique payloads. It is not an end-to-end
  `Repository` commit and is not directly comparable with the native SQLite
  repository benchmark.
- Scenario order is fixed, operating-system caches are not cleared, and browser
  durability ultimately depends on the browser and operating system.
- The test verifies normal close/reopen recovery, not process-kill or power-loss
  recovery. Crash-tail semantics remain covered by log-codec unit tests.
- Logical deletion and compaction are not benchmarked because compaction is not
  implemented.

## Reproduction

```powershell
npm install --prefix paper-workloads/opfs-benchmark
.\paper-workloads\run-opfs.ps1
```

The runner uses a same-origin local HTTP server, a module Dedicated Worker and
a real browser OPFS. It deletes benchmark files after every trial.
