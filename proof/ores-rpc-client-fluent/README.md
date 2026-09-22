# Cross-org proof: shared RPC fluent client lane

This directory is build evidence for the parallel `ores-http-clients` lane while the source `ORESoftware` org has no runnable GitHub Actions capacity.

It intentionally does **not** become a Fiducia product authority. `source-pins.json` records the exact authoritative source head and blob SHAs from `ORESoftware/ores-http-clients` PR #13 plus the merged `ORESoftware/ores-interfaces` contract. The proof fixtures mirror those pinned fluent surfaces in a small standalone harness so they can compile with `fiducia-cloud-test` GitHub Actions minutes.

The proof checks the terminal invariant across TypeScript, Dart, Rust and Gleam:

- unary execution: `makeCall()` / `make_call()`
- server-stream execution: `doStream()` / `do_stream()`
- `.stream()` / `stream(...)` is not an execution-terminal alias

Harness-only manifests and facade files exist solely to compile the pinned fluent surfaces in this org.
