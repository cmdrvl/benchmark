# Benchmark Fixture Corpus

Fixture naming follows the planned harness split:

- `BENCH_U###` fixtures support unit-level parser/refusal checks.
- `BENCH_I###` fixtures support integration-style candidate, key, lock, and report flows.
- `smoke/` contains the smallest deterministic happy-path fixtures for each supported candidate family.
- `refusal/` contains malformed or out-of-scope inputs that should refuse.
- `perf/` contains deterministic generator hooks for larger runtime coverage.
- `expected/` contains golden JSON report templates keyed by fixture set name, used for determinism gate validation.

## Current fixture families

### Smoke candidates (happy-path)

- `candidates/smoke/bench_i001_candidate.{csv,json,jsonl,parquet}` — minimal 2-row candidate for format coverage
- `candidates/smoke/bench_mixed.csv` — 5-row candidate for mixed pass/fail/skip scoring

### Smoke assertions (happy-path)

- `assertions/smoke/bench_i001_gold.jsonl` — 2 assertions against bench_i001 candidates (both should pass)
- `assertions/smoke/bench_mixed_gold.jsonl` — 7 assertions producing pass, fail, SKIP_ENTITY, and SKIP_FIELD outcomes
- `assertions/smoke/bench_all_skip_gold.jsonl` — 2 assertions referencing absent entities (all skip, accuracy=null)

### Refusal candidates (error paths)

- `candidates/refusal/bench_i004_nested.json` — nested JSON object (E_FORMAT_DETECT)
- `candidates/refusal/bench_no_comp_id.csv` — missing expected key column (E_KEY_NOT_FOUND)
- `candidates/refusal/bench_duplicate_key.csv` — duplicate key values (E_KEY_NOT_UNIQUE)
- `candidates/refusal/bench_null_key.csv` — null/blank key values (E_KEY_NULL)

### Refusal assertions (error paths)

- `assertions/refusal/bench_u001_malformed.jsonl` — malformed JSONL line (E_BAD_ASSERTIONS)
- `assertions/refusal/bench_bad_tolerance.jsonl` — tolerance on string mode (E_BAD_ASSERTIONS)
- `assertions/refusal/bench_empty.jsonl` — empty file (E_EMPTY_ASSERTIONS)

### Lock fixtures

- `locks/smoke/bench_i010_candidate.lock.json` — valid lockfile for bench_i001_candidate.csv
- `locks/refusal/bench_drift.lock.json` — lockfile with wrong hash (E_INPUT_DRIFT)
- `locks/refusal/bench_non_member.lock.json` — lockfile referencing a different file (E_INPUT_NOT_LOCKED)

### Perf fixtures

- `perf/generate_perf_smoke.sh` — deterministic generator for N-row candidate + assertion files

### Expected outputs

- `expected/bench_mixed_fail.json` — expected JSON report shape for mixed pass/fail/skip scoring
- `expected/bench_all_skip.json` — expected JSON report shape for all-skip scenario (accuracy=null)

## Naming conventions

- Fixture file names use `bench_` prefix for grep-friendliness
- Smoke fixtures use descriptive names: `bench_i001_candidate`, `bench_mixed`, `bench_all_skip`
- Refusal fixtures describe the failure mode: `bench_duplicate_key`, `bench_null_key`, `bench_drift`
- Expected outputs match their fixture set name: `bench_mixed_fail.json`, `bench_all_skip.json`
