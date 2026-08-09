# Persistent-tree structural growth artifact

Generated: 2026-08-09  
Command: `cargo run --release --offline --example paper_artifact`  
Source base commit: `a6ff2d5befa061a9af758aefb0e792ac9b0b4776` plus the uncommitted format-1 repair patch  
Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`, `cargo 1.97.1`  
Host: Windows NT 10.0.26200.0, x86-64, Intel Core i9-13900H (14 cores/20 logical processors)  
Metadata filesystem: NTFS  

The object graph itself was stored in the verified in-memory backend. Repository references and locks used the host filesystem. Counts and canonical bytes therefore exclude filesystem allocation units, directory entries and reference files.

Parameters: N=1,024 coordinates, R=50 incremental saves, 256-byte payloads. Every initial and edited payload is globally unique. Each round rotates the edited coordinate window while preserving the coordinate set.

| k | blobs | branches | leaves | commits | total objects | canonical bytes | loose upper bound |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1,074 | 9,066 | 1,074 | 51 | 11,265 | 804,461 | 19,383 |
| 10 | 1,524 | 12,694 | 1,524 | 51 | 15,793 | 1,109,269 | 27,483 |
| 100 | 6,024 | 48,991 | 6,024 | 51 | 61,090 | 4,166,461 | 108,483 |

The loose bound is

```text
18N + 1 + R(18k + 1)
```

because the initial graph contains at most N Blobs, N leaves, 16N branch nodes and one Commit; each incremental save adds at most k Blobs, k leaves, 16k branch nodes and one Commit. Shared prefixes and content reuse make the measured count lower. This is an upper bound, not an exact closed form.

## Reproduction notes

- The generator asserts uniqueness through construction: the version and coordinate index occupy the first 16 payload bytes.
- The experiment classifies every stored object by decoding its canonical type tag.
- Hash verification is active for all reads and reuse checks.
- Run the command above from a clean checkout of the artifact revision; do not compare these counts with the former flat-tree formula.
