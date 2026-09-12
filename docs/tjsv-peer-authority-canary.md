# TJSV peer-authority canary

This `fiducia-cloud-test` repository intentionally keeps two independently authored contract sources under `contracts/tjsv-canary/`:

- `main.tsp` — human-authored TypeSpec;
- `authored.schema.json` — human-authored JSON Schema Draft 2020-12.

The workflow pins `ORESoftware/typespec-json-schema-validator` to an immutable, tested commit. TJSV compiles the TypeSpec lane into `typespec.generated.schema.json` inside ignored `tmp/` evidence, then compares that generated Schema B with authored Schema A. The generated schema is evidence only and is never committed or promoted to authority.

The test requires positive structural/differential parity, explicitly replays `tjsv compare`, then mutates only a temporary copy of authored Schema A and requires `STOPPED_FOR_EVALUATION`. It also requires a missing authored peer to fail closed. Source hashes and Git diffs prove the authored inputs are not rewritten.

This canary certifies the dual-authority workflow shape in a `*-test` org. It does not claim universal semantic equivalence, production deployment, or permission to replace either authored lane with generated output.
