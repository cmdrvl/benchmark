# Perf Smoke Baseline

Use the deterministic fixture generator and the integrated perf smoke test together:

```bash
tests/fixtures/perf/generate_perf_smoke.sh /tmp/benchmark-perf 512
cargo test BENCH_I901_perf_smoke_integrated_path_captures_phase_timings -- --nocapture
```

Baseline procedure:

1. Run from a warm build directory after `cargo test` has already compiled the crate.
2. Keep the generated row count at `512` so timings remain comparable across changes.
3. Record the printed `candidate_load_ms`, `scoring_ms`, `render_ms`, and `execute_total_ms`.
4. Compare later runs against the same machine class before tightening guardrails.
