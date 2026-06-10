# Benchmark Ergonomics Handoff

Pass 1 applied top-level agent entrypoints and release formula hygiene for `benchmark`.

Validated runtime surfaces before full gates:

- `benchmark --robot-triage`
- `benchmark capabilities --json`
- `benchmark --json`
- `benchmark robot-docs guide`
- `benchmark`
- `benchmark --jsno`

Next pass focus:

- Re-score after the `v0.3.0` release lands in `cmdrvl/tap/cmdrvl-benchmark`.
- Consider updating GitHub Actions dependencies ahead of the Node 20 retirement window.
