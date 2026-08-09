# Format-2 benchmark summary

Generated: 2026-08-09  
Benchmark: `benches/storage.rs`, Criterion 0.8.2  
Commands: filtered `cargo bench --offline --bench storage -- <group>` invocations
Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`, release profile  
Host: Windows NT 10.0.26200.0, x86-64, Intel Core i9-13900H (14 cores/20 logical processors)  
Filesystem: NTFS on drive D  
Criterion configuration: 1 s warm-up, 3 s target measurement, 10 samples

Values are bootstrap median point estimates with Criterion's median confidence interval. Payloads are 256 bytes, exact world sizes are asserted, and every initial payload is unique. Repository/database initialization occurs in Criterion fixture setup outside the timed commit closure for both SQLite and loose stores.

## Verified memory backend

This isolates canonical encoding, hashing and persistent-tree construction from durable storage.

| Operation | N | Median | Median CI |
| --- | ---: | ---: | ---: |
| full snapshot | 100 | 8.228 ms | 6.944–10.545 ms |
| full snapshot | 1,000 | 22.936 ms | 20.849–24.464 ms |
| full snapshot | 10,000 | 147.431 ms | 137.209–149.577 ms |

## Default SQLite backend

`SqliteStore` stores verified canonical objects in one `WITHOUT ROWID` table. All objects for a repository commit are inserted under one `BEGIN IMMEDIATE` transaction using rollback journaling and `synchronous=FULL`; the transaction commits before ref publication.

| Operation | N | Median | Median CI |
| --- | ---: | ---: | ---: |
| full snapshot | 100 | 14.021 ms | 12.771–15.519 ms |
| full snapshot | 1,000 | 47.433 ms | 43.643–50.075 ms |
| full snapshot | 10,000 | 1,156.306 ms | 1,016.622–1,246.116 ms |
| incremental commit, k=1 | 100 | 10.446 ms | 10.268–12.132 ms |
| incremental commit, k=1 | 1,000 | 13.577 ms | 12.465–14.660 ms |
| full load | 100 | 21.221 ms | 20.831–21.956 ms |
| full load | 1,000 | 222.167 ms | 216.701–231.314 ms |
| logical checkout | 100 | 5.225 ms | 5.147–5.425 ms |
| logical checkout | 1,000 | 7.765 ms | 7.351–8.507 ms |

## Same-revision loose-file control

The explicit `FilesystemStore` retains one verified file per object. At N=1,000 its full-snapshot median is 10,427.392 ms [6,265.793,10,953.676]. The wide interval reflects the high and variable cost of creating thousands of NTFS files.

SQLite's 47.433 ms median is approximately **220× faster** than this same-revision loose-file median. It also improves on format 1's archived 10,763.069 ms result. The speedup comes from replacing thousands of create/write/rename operations with indexed inserts under one durable transaction; the radix algorithm and canonical objects are unchanged.

## Naive raw-file baseline

The earlier baseline creates one raw payload file per coordinate and provides no typed objects, history graph, verified reads or ref semantics.

| N | Median | Median CI |
| ---: | ---: | ---: |
| 100 | 18.048 ms | 17.320–28.290 ms |
| 1,000 | 299.824 ms | 262.367–321.357 ms |

At N=1,000, the default SQLite CAS is about 6.3× faster than this feature-incomplete raw-file baseline on the measured host. This comparison is operational, not semantic equivalence.

## Interpretation constraints

- The 10,000-point SQLite result has a wider interval because Criterion could collect only one iteration per sample within the configured run.
- Incremental, load and checkout groups build one fixture outside the timed loop and reuse it.
- Memory results were not rerun because the object algorithm did not change; they remain the format-1 algorithm control.
- Absolute results are from one Windows/NTFS machine and do not establish cross-platform performance.
- SQLite removes the dominant initial-import bottleneck, but N=10,000 remains roughly 7.8× slower than the memory algorithm, leaving query/insert and durability overhead for future profiling.
