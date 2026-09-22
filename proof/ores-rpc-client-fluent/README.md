# Cross-org proof: shared RPC fluent client lane

This directory is build evidence for the parallel `ores-http-clients` lane while the source `ORESoftware` org has no runnable GitHub Actions capacity.

It intentionally does **not** become a Fiducia product authority. The runtime snapshots are copied byte-for-byte from `ORESoftware/ores-http-clients` PR #13 head `797e9c7a1715188372ec2a86161985940fb50fe4`; the contract snapshots are copied from merged `ORESoftware/ores-interfaces` PR #48 merge commit `a071dbd727418b0483c3d514c862c0bb4f8633c0`.

The proof checks the terminal invariant across TypeScript, Dart, Rust and Gleam:

- unary execution: `makeCall()` / `make_call()`
- server-stream execution: `doStream()` / `do_stream()`
- `.stream()` / `stream(...)` is not an execution-terminal alias

Harness-only manifests and facade files exist solely so the exact snapshots can compile and test in this org.
