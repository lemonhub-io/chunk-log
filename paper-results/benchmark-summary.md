# Format-1 benchmark summary

Generated: 2026-08-09  
Benchmark: `benches/storage.rs`, Criterion 0.8.2  
Commands: filtered invocations of `cargo bench --offline --bench storage -- <group>`  
Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`, release profile  
Host: Windows NT 10.0.26200.0, x86-64, Intel Core i9-13900H (14 cores/20 logical processors)  
Filesystem: NTFS on drive D  
Criterion configuration: 1 s warm-up, 3 s target measurement, 10 samples  

Values are bootstrap median point estimates with the median confidence interval reported by Criterion. Payloads are 256 bytes, exact world sizes are asserted, and every initial payload is unique by construction.

## Algorithm backend

The verified in-memory backend isolates canonical encoding, hashing and persistent-tree construction from per-file directory operations. Repository refs still use a temporary metadata directory.

| Operation | N | Median | Median CI |
| --- | ---: | ---: | ---: |
| full snapshot | 100 | 8.228 ms | 6.944–10.545 ms |
| full snapshot | 1,000 | 22.936 ms | 20.849–24.464 ms |
| full snapshot | 10,000 | 147.431 ms | 137.209–149.577 ms |

## Filesystem backend

The filesystem backend stores every canonical object as a separate verified file. It uses atomic namespace publication but no per-object sudden-power-loss barrier.

| Operation | N | Median | Median CI |
| --- | ---: | ---: | ---: |
| full snapshot | 100 | 970.408 ms | 636.807–1,069.092 ms |
| full snapshot | 1,000 | 10,763.069 ms | 10,063.793–11,356.041 ms |
| incremental commit, k=1 | 100 | 27.191 ms | 25.371–30.515 ms |
| incremental commit, k=1 | 1,000 | 28.943 ms | 25.778–31.354 ms |
| full load | 100 | 47.257 ms | 43.179–48.955 ms |
| full load | 1,000 | 608.815 ms | 549.815–682.236 ms |
| logical checkout | 100 | 8.099 ms | 7.797–8.590 ms |
| logical checkout | 1,000 | 7.945 ms | 7.107–8.708 ms |

The k=1 result is approximately independent of N over the tested range because it republishes one fixed-depth radix path. The initial filesystem import is much slower than the naive baseline because format 1 creates multiple small Merkle-node files per coordinate. This is a material limitation of the file-per-object backend, not measurement noise.

## Naive full-snapshot baseline

Each measured iteration creates a fresh directory and writes one raw payload file per coordinate. It does not provide typed objects, history graph, verified reads or reference semantics, so it is a storage-cost baseline rather than a feature-equivalent system.

| N | Median | Median CI |
| ---: | ---: | ---: |
| 100 | 18.048 ms | 17.320–28.290 ms |
| 1,000 | 299.824 ms | 262.367–321.357 ms |

## Interpretation constraints

- Full-snapshot and naive results include fresh-directory publication in every measured sample.
- Incremental, load and checkout groups build one fixture outside the timed loop and reuse it.
- The time required to construct large filesystem fixtures is not part of incremental/load/checkout samples, but it materially increases the wall-clock reproduction time.
- The 10,000-coordinate filesystem fixture is intentionally excluded from the default suite because the file-per-node backend creates an impractical number of small files. Algorithm scaling to 10,000 is reported through the verified memory backend.
- These results come from one Windows/NTFS machine and do not establish cross-platform absolute performance.
- A packfile or database object backend is required before claiming competitive initial-import performance.
