# Reproduction workloads

This directory contains auxiliary fixtures used to reproduce measurements in
the paper. It is deliberately isolated from the Rust crate and is not required
to build, test, or use `chunklog`.

- `opfs-benchmark/` is a small browser harness. JavaScript and HTML are needed
  to start Chromium, create a dedicated worker, and call the WebAssembly build.
- `luanti-game/` and `luanti-world/` are Luanti fixtures. The Lua entry point is
  loaded by Luanti itself.
- `run-opfs.ps1` and `run-luanti.ps1` are Windows reproduction entry points.

These files are marked `linguist-detectable=false` in the root
`.gitattributes`. GitHub's language bar therefore describes the shipped Rust
implementation instead of the external environments used to measure it.

Generated measurements belong in `../paper-results/`; product code belongs in
`../src/`, with Rust tests, examples, and benchmarks in their existing root
directories.
