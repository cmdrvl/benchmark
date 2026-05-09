# benchmark

**Gold-set scoring for extraction pipelines.**

`benchmark` is the epistemic spine tool for checking a row-oriented candidate dataset against a human-validated assertion set. It answers a narrow but important question:

**Did this extraction produce the facts we expected, and if not, are the misses wrong values or missing values?**

Current status:

- repository status: implemented Rust crate with validated benchmark v0 command path
- source of truth: [docs/PLAN_BENCHMARK.md](./docs/PLAN_BENCHMARK.md)
- current repo contents: plan + Beads execution graph + working Rust crate + fixture and perf corpus
- release status: `v0.2.1` is published and available via the `cmdrvl-benchmark` Homebrew tap formula

The examples below describe the implemented `v0` contract. The crate is runnable locally, CI and release automation now live in-repo, and the current release binary already follows the same contract.

---

## Current quickstart

The current quickstart is for contributors and local operators:

```bash
cd benchmark
cargo run -- --help
sed -n '1,260p' docs/PLAN_BENCHMARK.md
br ready
cargo build --release
cargo run -- \
  tests/fixtures/candidates/smoke/bench_i001_candidate.csv \
  --assertions tests/fixtures/assertions/smoke/bench_i001_gold.jsonl \
  --key comp_id \
  --json
cargo run -- \
  tests/fixtures/candidates/smoke/bench_mixed.csv \
  --assertions tests/fixtures/assertions/smoke/bench_mixed_gold.jsonl \
  --key comp_id \
  --render summary
cargo test
```

If you are here to build or verify the tool, start with the plan, the Beads graph, and the local fixture corpus.

Read-only doctor mode is available for automation and agent triage:

```bash
benchmark doctor health --json
benchmark doctor capabilities --json
benchmark doctor robot-docs
benchmark doctor --robot-triage
```

Doctor mode does not read candidates, assertions, lockfiles, fixture paths, stdin, or DuckDB state. It does not score assertions, verify locks, mint gold truth, write artifacts, or provide a `--fix` surface.

---

## Why benchmark exists

Most extraction evaluation breaks down in one of two ways:

- you diff two files that are not actually the same shape
- you collapse "wrong value" and "value missing entirely" into one blunt score

`benchmark` is designed to avoid both mistakes.

You provide:

- one candidate artifact that materializes to one row-oriented relation
- one JSONL assertion file of `(entity, field, expected)` facts
- one stable key column

`benchmark` returns:

- row-level `PASS`, `FAIL`, and `SKIP` outcomes
- separate `accuracy` and `coverage`
- a derived `quality_band` for downstream `assess` policies
- deterministic JSON and human-readable reports
- optional lock verification so scoring happens against trusted inputs

---

## What makes this different

- **Gold-set scoring, not diffing**. `benchmark` scores candidate outputs against expected facts, even when there is no aligned "old vs new" file pair.
- **Missingness stays separate from correctness**. If an entity or field is absent, that is a skip, not a silent failure and not a fake pass.
- **Comparison semantics are explicit**. Assertions declare `compare_as`; the tool never guesses numeric or date semantics from formatting accidents.
- **Deterministic reports**. Same candidate bytes plus same assertions bytes should produce the same ordered report.
- **Spine boundary discipline**. `benchmark` scores against gold truth, may emit a derived quality signal for downstream policy, and still does not resolve entities, validate business rules, or make proceed/block decisions.

---

## Where benchmark fits

`benchmark` sits in the scoring layer of the spine:

```text
extract / normalize -> benchmark -> assess -> pack
```

Related tools:

| If you need... | Use |
|----------------|-----|
| Structural comparability of two aligned datasets | [`shape`](https://github.com/cmdrvl/shape) |
| Numeric/content deltas between aligned datasets | [`rvl`](https://github.com/cmdrvl/rvl) |
| Constraint validation against declared rules | `verify` |
| Proceed / escalate / block policy decisions | `assess` |
| Evidence sealing | [`pack`](https://github.com/cmdrvl/pack) |

`benchmark` only answers:

**Does the candidate satisfy the expected facts in the gold set, and what is the score?**

---

## How benchmark compares

| Capability | benchmark | `rvl` | `verify` | manual spot-checking |
|------------|-----------|-------|----------|----------------------|
| Scores against gold facts | ✅ | ❌ | ❌ | ⚠️ human-only |
| Separates wrong from missing | ✅ | ❌ | ❌ | ⚠️ inconsistent |
| Explicit comparison semantics | ✅ | ⚠️ profile/equivalence-based | ✅ rule-based | ❌ |
| Deterministic JSON report | ✅ | ✅ | ✅ | ❌ |
| Good for tournament ranking | ✅ | ❌ | ⚠️ gate/penalty only | ❌ |

Use `benchmark` when the question is "did we extract the expected facts?" not "what changed between two aligned datasets?" and not "did declared constraints hold?"

---

## What benchmark is not

`benchmark` is not:

- a diff tool
- a validator
- a profiler
- an entity-resolution system
- an automatic gold-set generation tool

Fresh-eyes boundary that matters:

- approved evidence is **not** automatically ground truth
- any future harvest helper must remain draft-only and require human review

---

## Current v0 workflow

```bash
benchmark normalized.csv --assertions gold_set.jsonl --key comp_id --json
```

Current JSON shape:

```json
{
  "tool": "benchmark",
  "version": "benchmark.v0",
  "outcome": "FAIL",
  "candidate": "normalized.csv",
  "candidate_hash": "sha256:1a2b3c...",
  "assertions_file": "gold_set.jsonl",
  "assertions_hash": "sha256:f1e2d3...",
  "key_column": "comp_id",
  "input_verification": null,
  "policy_signals": {
    "quality_band": "LOW",
    "quality_band_basis": "assertion_failures_present"
  },
  "summary": {
    "total": 216,
    "passed": 214,
    "failed": 1,
    "skipped": 1,
    "resolved": 215,
    "accuracy": 0.995,
    "coverage": 0.995,
    "by_severity": {
      "critical": { "passed": 0, "failed": 0, "skipped": 0 },
      "major": { "passed": 214, "failed": 1, "skipped": 1 },
      "minor": { "passed": 0, "failed": 0, "skipped": 0 }
    }
  },
  "failures": [
    {
      "entity": "comp_3",
      "field": "u8:adj_location",
      "expected": "5.0%",
      "actual": "5.5%",
      "compare_as": "percent",
      "tolerance": 0.01,
      "severity": "major",
      "source": "reference_excel:E18"
    }
  ],
  "skipped": [
    {
      "entity": "comp_7",
      "field": "u8:cap_rate",
      "reason": "SKIP_ENTITY",
      "detail": "Entity 'comp_7' not found in candidate"
    }
  ],
  "refusal": null
}
```

The policy signal is intentionally coarse and fully derived from the raw score:

- `HIGH` = no failures and no skips
- `ACCEPTABLE` = no failures but one or more skips
- `LOW` = one or more failed assertions

Tournament ranking still uses `summary.accuracy` and `summary.coverage`. `assess` can consume `policy_signals.quality_band` without becoming a numeric-threshold engine.

Current human output:

```text
BENCHMARK FAIL
candidate: normalized.csv
assertions: gold_set.jsonl
key: comp_id
passed: 214
failed: 1
skipped: 1
quality_band: LOW (assertion_failures_present)
accuracy: 0.995
coverage: 0.995

FAIL comp_3 u8:adj_location expected=5.0% actual=5.5% compare_as=percent tolerance=0.01
SKIP comp_7 u8:cap_rate reason=SKIP_ENTITY
```

Operator summary surfaces:

```text
tool=benchmark version=benchmark.v0 candidate=normalized.csv outcome=FAIL accuracy=0.995 coverage=0.995 failed=1 skipped=1 quality_band=LOW refusal_code=-
```

```tsv
tool	version	candidate	outcome	accuracy	coverage	failed	skipped	quality_band	refusal_code
benchmark	benchmark.v0	normalized.csv	FAIL	0.995	0.995	1	1	LOW	-
```

Use them via:

```bash
benchmark normalized.csv --assertions gold_set.jsonl --key comp_id --render summary
benchmark normalized.csv --assertions gold_set.jsonl --key comp_id --render summary-tsv
```

Successful `PASS` and `FAIL` scoring runs are expected to keep `stderr` empty. Native dependency noise is suppressed on those paths so stdout remains the complete operator artifact; only refusal-path or unexpected top-level diagnostics should surface outside the report.

---

## The three outcomes

`benchmark` should emit exactly one domain outcome:

| Exit | Outcome | Meaning |
|------|---------|---------|
| `0` | `PASS` | all assertions satisfied, no skips |
| `1` | `FAIL` | one or more assertions failed or skipped |
| `2` | `REFUSAL` | the tool could not score safely |

Why skips are exit `1`:

- a candidate can be internally fine and still fail to cover the gold set
- automation should not treat missing benchmarked facts as a clean success

---

## Candidate contract

`benchmark v0` scores exactly one **row-oriented relation** at a time.

That means:

- one stable key column named by `--key`
- scalar fields addressable by column name
- one row per entity

Supported candidate formats:

| Format | Status in v0 |
|--------|--------------|
| CSV | supported |
| JSONL | supported |
| Parquet | supported |
| row-oriented JSON | supported |
| document-shaped / nested JSON | out of scope for v0 |

Important non-goal:

- `benchmark` must never guess how to flatten a document-shaped input

---

## Assertion contract

Target assertion format is JSONL, one assertion per line:

```jsonl
{"entity": "comp_4", "field": "u8:cap_rate", "expected": "6.76%", "compare_as": "percent", "tolerance": 0.01, "severity": "critical", "source": "reference_excel:D14"}
{"entity": "comp_4", "field": "u8:sale_price", "expected": "28200000", "compare_as": "number", "tolerance": 1000, "severity": "major", "source": "reference_excel:D5"}
{"entity": "comp_1", "field": "u8:property_name", "expected": "Marquis at Briarcliff", "compare_as": "string", "severity": "major", "source": "reference_excel:B3"}
```

Fields:

| Field | Meaning |
|-------|---------|
| `entity` | row key value to match against `--key` |
| `field` | candidate column name |
| `expected` | expected value |
| `compare_as` | `string`, `number`, `percent`, or `date` |
| `tolerance` | absolute tolerance for `number` or `percent` only |
| `severity` | `critical`, `major`, or `minor` |
| `source` | provenance for the gold fact |

---

## Score semantics

`benchmark` keeps correctness and missingness separate:

- `resolved = passed + failed`
- `accuracy = passed / resolved`
- `coverage = resolved / total`

If `resolved = 0`, then:

- `accuracy = null`
- this is a domain failure, not a refusal

That distinction matters in tournament and factory settings. A system that never found any benchmarked facts should not look like a system that found facts and got them all wrong.

`benchmark` also exposes a derived policy-facing classification:

- `quality_band = HIGH | ACCEPTABLE | LOW`
- `quality_band_basis` explains which deterministic rule produced that band

That keeps the scoring basis auditable while giving `assess` a small exact-match surface.

---

## Comparison modes

Supported `compare_as` modes:

| Mode | Semantics |
|------|-----------|
| `string` | exact string match after whitespace trimming |
| `number` | numeric comparison with optional absolute tolerance |
| `percent` | percentage-point text comparison with optional absolute tolerance |
| `date` | normalized canonical date comparison |

Two important constraints:

- tolerance is only legal for `number` and `percent`
- `percent` does **not** auto-convert ratio-form decimals like `0.0676` into `6.76%`

---

## Refusals

Current refusal codes:

| Code | Meaning |
|------|---------|
| `E_IO` | candidate or assertions file unreadable |
| `E_BAD_ASSERTIONS` | malformed or semantically invalid assertions |
| `E_KEY_NOT_FOUND` | key column absent |
| `E_KEY_NOT_UNIQUE` | duplicate benchmark key values |
| `E_KEY_NULL` | null or blank benchmark key values |
| `E_FORMAT_DETECT` | unsupported or non-row-oriented candidate input |
| `E_EMPTY_ASSERTIONS` | zero valid assertions |
| `E_INPUT_NOT_LOCKED` | candidate not present in provided lockfile(s) |
| `E_INPUT_DRIFT` | candidate hash mismatch against lock member |

Refusals are part of the contract, not generic exceptions.

---

## Lock verification

`benchmark` can optionally verify the candidate against one or more lockfiles before scoring:

```bash
benchmark normalized.csv --assertions gold.jsonl --key comp_id \
  --lock normalized.lock.json --json
```

Current behavior:

- if the candidate is not in the provided lockfile set: refuse
- if the candidate hash drifts from the lock member: refuse
- if verification passes: record that in `input_verification`

This preserves the spine rule that scoring should happen on trusted inputs, not on silently drifting local files.

---

## Tournament and factory use

`benchmark` is not just a developer CLI. It is the spine scoring primitive for:

- extraction bakeoffs
- tournament evaluation
- quality gating before `assess`
- evidence generation before `pack`

Target workflow:

```bash
benchmark normalized.csv --assertions gold.jsonl --key comp_id --json > benchmark.report.json
assess benchmark.report.json --policy extraction_quality.v1 > decision.json
pack seal benchmark.report.json normalized.csv gold.jsonl --output evidence/scored/
```

In tournament mode, the ranking signal is:

- primary: `summary.accuracy`
- tie-breaker: `summary.coverage`

`assess` may gate on `policy_signals.quality_band`, but ranking should continue to use the raw summary metrics.

---

## Repository status

Today this repo contains:

- the detailed plan: [docs/PLAN_BENCHMARK.md](./docs/PLAN_BENCHMARK.md)
- the Beads execution graph in `.beads/`
- an implemented Rust crate in `src/`
- fixtures and integration/perf coverage in `tests/`

Current implementation status:

- plan quality: implementation-grade and now reflected in code
- README and AGENTS: now explicit and repo-specific
- code: implemented crate with CLI orchestration, scoring, rendering, lock checks, and perf smoke coverage
- release surface: CI workflow, release workflow, and Homebrew-tap update path now exist; no published release binary yet

Immediate next step in the repo:

- keep docs aligned with the shipped crate surface
- preserve determinism, named BENCH coverage, and perf smoke guardrails as changes land

---

## Repo structure

The crate currently uses this layout:

| Path | Role |
|------|------|
| `src/lib.rs` | module root and shared library surface |
| `src/main.rs` | thin CLI entrypoint |
| `src/cli.rs` | argument parsing and mode wiring |
| `src/assertions.rs` | assertion parsing and validation |
| `src/candidate.rs` | candidate loading and format detection |
| `src/key_check.rs` | key existence / uniqueness / null checks |
| `src/compare.rs` | comparison semantics |
| `src/engine.rs` | assertion evaluation and outcomes |
| `src/lock_check.rs` | lock verification |
| `src/report.rs` | internal report model and summary math |
| `src/render.rs` | human and JSON output rendering |
| `src/refusal.rs` | refusal taxonomy and envelopes |
| `tests/fixtures/` | shared candidate / assertions / lock fixtures |
| `tests/cli.rs` | CLI mode and end-to-end contract tests |
| `tests/scoring_matrix.rs` | scoring and comparison matrix tests |
| `tests/refusals.rs` | refusal-path coverage |
| `tests/lock_integration.rs` | lock verification coverage |
| `tests/perf_smoke.rs` | runtime smoke guardrails |

---

## Contributing right now

If you are working in this repo:

1. read [docs/PLAN_BENCHMARK.md](./docs/PLAN_BENCHMARK.md)
2. read [AGENTS.md](./AGENTS.md)
3. inspect ready Beads work with `br ready`
4. run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `ubs .`
5. implement only behavior already specified in the plan

Current work should improve one of:

- contract fidelity
- runtime safety
- test and perf guardrails
- documentation and release hygiene

---

## Roadmap

Near-term:

- keep README and AGENTS aligned with the actual crate
- preserve named quality gates and runtime smoke checks
- cut the first tagged release after the next explicit version bump

Deferred by design:

- document-shaped candidate scoring
- multi-relation semantics
- auto-harvest into the gold set
- policy logic that belongs in `assess`

---

## Source of truth

If the README and the plan ever disagree, follow:

1. [docs/PLAN_BENCHMARK.md](./docs/PLAN_BENCHMARK.md)
2. [AGENTS.md](./AGENTS.md)
3. this README
