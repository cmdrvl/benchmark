# benchmark — Extraction Accuracy Scoring

## One-line promise
**Check whether a dataset satisfies a set of expected (entity, field, value) assertions — scoring extraction quality against a human-validated reference.**

---

## Decision

`benchmark` is the spine's gold-set scoring primitive.

It evaluates one candidate relation against a versioned assertion set and emits a deterministic scoring report. It does not resolve entities, infer truth from prior outputs, or make proceed/block decisions.

---

## Problem

You extract data from PDFs, Excel files, or SEC filings. How do you know the extraction is correct? You can't `rvl` the output against the reference — they're different shapes. The reference is a human's notes, a spot-check spreadsheet, a set of facts scattered across documents.

`benchmark` solves the **cross-shape comparison problem**. The reference isn't a dataset. It's a set of claims about what values should exist. Each assertion is schema-independent — `(comp_4, cap_rate, 6.76%)` doesn't care about column order, CSV structure, or file format. The assertion just needs to locate the value in the candidate and check it.

---

## Non-goals

`benchmark` is NOT:
- A diff tool (that's `rvl` for aligned datasets)
- A validator (that's `verify`)
- A profiler (that's `shape`)

It does not transform data. It scores whether extracted data matches expected facts.

## V0 scope discipline

V0 is intentionally narrow:

- one candidate artifact
- one assertion file
- one row-oriented relation
- one key column
- one deterministic score report

Deferred beyond v0:

- document-shaped candidate evaluation
- multi-relation joins as first-class input semantics
- automatic gold-set generation
- policy decisions (`assess` owns proceed/escalate/block)
- benchmark packs or hosted orchestration concerns

---

## Non-negotiables

These are engineering contracts, not aspirations. If any are violated, `benchmark` is not `benchmark` yet.

1. Explicit comparison semantics only. `benchmark` never infers numeric/date/percent semantics from formatting accidents.
2. No ambiguous entity lookup. Duplicate or null benchmark keys refuse the run.
3. Missingness is not failure. Missing rows/fields become skips, not silent passes and not fabricated values.
4. Accuracy and coverage stay separate. The tool never compresses them into one score.
5. Ground truth is imported, not minted. `benchmark` consumes human-validated assertions and does not promote prior outputs into truth.
6. Reports are deterministic. Same bytes in candidate and assertions produce the same ordered report.

---

## CLI

```
benchmark <CANDIDATE> --assertions <FILE> [OPTIONS]

Arguments:
  <CANDIDATE>            File to score (CSV, JSON, JSONL, or Parquet)

Options:
  --assertions <FILE>    Assertion file (JSONL)
  --key <COLUMN>         Key column for entity lookup in candidate
  --lock <LOCKFILE>      Verify candidate is a member of these lockfiles (repeatable)
  --json                 JSON output
```

### Exit codes

`0` PASS (all assertions satisfied, no skips) | `1` FAIL (one or more assertions failed or skipped) | `2` refusal

### Tool category

`benchmark` is a **report tool**.

- default stdout: human-readable summary
- `--json`: machine-readable full report
- stderr: process diagnostics only, never evidence

## Candidate contract

`benchmark` scores one **row-oriented relation** at a time.

That means v0 candidates must materialize to a single table-like surface with:

- one stable key column named by `--key`
- scalar fields addressable by column name
- one row per entity

CSV, JSONL, Parquet, and row-shaped JSON are fine when DuckDB can materialize them this way. Nested JSON objects, arrays-of-arrays, or document-shaped blobs that do not expose a single scalar relation are out of scope for v0 and should be normalized before scoring.

## Module skeleton (implementation target)

`benchmark` should start as a single Rust binary crate with explicit modules:

- `src/main.rs` — CLI entrypoint, exit code mapping, top-level orchestration
- `src/cli.rs` — `clap` argument parsing and early flag validation
- `src/assertions.rs` — assertion JSONL parsing, schema validation, stable ordering
- `src/candidate.rs` — candidate format detection and DuckDB relation loading
- `src/key_check.rs` — key existence, uniqueness, and null/blank validation queries
- `src/compare.rs` — `string` / `number` / `percent` / `date` comparison semantics
- `src/engine.rs` — deterministic join/projection evaluation from assertions to outcomes
- `src/lock_check.rs` — optional lock membership and hash verification
- `src/report.rs` — report structs, summary math, failure/skip materialization
- `src/render.rs` — human output and JSON serialization
- `src/refusal.rs` — refusal enum, recovery text, refusal envelope rendering

Test layout should be explicit too:

- `tests/fixtures/` — candidate files, assertion files, lockfiles, expected reports
- `tests/cli.rs` — CLI exit-code and mode tests
- `tests/scoring_matrix.rs` — comparison-mode and skip/fail scoring tests
- `tests/refusals.rs` — malformed input and contract refusal tests
- `tests/lock_integration.rs` — `--lock` happy-path and drift-path tests

Dependency direction:

- `cli` -> `candidate` / `assertions` / `lock_check` / `engine` / `render`
- `engine` -> `key_check` / `compare` / `report`
- `render` and `refusal` never reach back into DuckDB loading

---

## Assertion file format (JSONL)

Each line is an independent assertion — an expected `(entity, field, value)` tuple plus comparison metadata:

```jsonl
{"entity": "comp_4", "field": "u8:cap_rate", "expected": "6.76%", "compare_as": "percent", "tolerance": 0.01, "severity": "critical", "source": "reference_excel:D14"}
{"entity": "comp_4", "field": "u8:sale_price", "expected": "28200000", "compare_as": "number", "tolerance": 1000, "severity": "major", "source": "reference_excel:D5"}
{"entity": "comp_1", "field": "u8:property_name", "expected": "Marquis at Briarcliff", "compare_as": "string", "severity": "major", "source": "reference_excel:B3"}
```

| Field | Required | Description |
|-------|----------|-------------|
| `entity` | yes | Row key value — matched against `--key` column in candidate |
| `field` | yes | Column name (canonical ID after canon) |
| `expected` | yes | Expected value (string representation) |
| `compare_as` | no | Comparison mode: `string`, `number`, `percent`, or `date`. Default: `string` |
| `tolerance` | no | Absolute tolerance for `number` or `percent` comparisons |
| `severity` | no | Assertion importance: `critical`, `major`, or `minor`. Default: `major` |
| `source` | no | Provenance of the benchmark fact (where the human found this value) |

---

## Lookup mechanics

Benchmark loads the assertions JSONL and the candidate into DuckDB, validates the candidate key once, then evaluates assertions via deterministic joins and projection queries. Format is auto-detected from candidate extension.

### Key discipline

The candidate key column must:

- exist
- be unique for benchmarked rows
- contain no null/blank values for benchmarked rows

`benchmark` is not entity resolution. If the candidate cannot support unambiguous lookup, the tool should refuse rather than guess.

### Value comparison

- `string`: exact string match after whitespace trimming
- `number`: parse both values as numeric and compare with optional absolute tolerance
- `percent`: parse percentage-point text with optional absolute tolerance; v0 does not auto-convert ratio-form decimals like `0.0676` into `6.76%`
- `date`: parse canonical date text and compare normalized `YYYY-MM-DD`

If an assertion declares an invalid `compare_as` / `tolerance` combination, `benchmark` refuses with `E_BAD_ASSERTIONS`.

### Assertion outcomes

| Outcome | Meaning |
|---------|---------|
| `PASS` | Value matches (exact or within tolerance) |
| `FAIL` | Value doesn't match |
| `SKIP_ENTITY` | Entity key not found in candidate |
| `SKIP_FIELD` | Field not found in candidate |

### Summary metrics

- `resolved = passed + failed`
- `accuracy = passed / resolved`
- `coverage = resolved / total`

If `resolved = 0`, `accuracy` should be `null` rather than `0`. That distinguishes "nothing benchmarkable was found" from "everything benchmarkable was wrong."

## Data model invariants

- `I01` Candidate relation invariant: v0 operates on exactly one row-oriented relation.
- `I02` Key existence invariant: `--key` must resolve to one concrete candidate column before scoring begins.
- `I03` Key uniqueness invariant: each benchmarked `entity` maps to at most one candidate row.
- `I04` Key completeness invariant: null or blank key values in benchmarked rows are a refusal, not a skip.
- `I05` Field lookup invariant: `field` addresses candidate columns by exact canonical name; no fuzzy matching.
- `I06` Comparison explicitness invariant: every assertion is evaluated under exactly one declared or default `compare_as` mode.
- `I07` Tolerance validity invariant: tolerance is only legal for `number` and `percent`.
- `I08` Missingness invariant: missing entities and missing fields produce skips only.
- `I09` Summary math invariant: `resolved = passed + failed`, `skipped = total - resolved`.
- `I10` Accuracy nullability invariant: `accuracy` is `null` when `resolved = 0`.
- `I11` Input integrity invariant: when `--lock` is provided, lock verification must pass before scoring proceeds.
- `I12` Deterministic ordering invariant: failures and skips are emitted in stable assertion order.

---

## Output (JSON)

```json
{
  "version": "benchmark.v0",
  "outcome": "FAIL",
  "candidate": "normalized.csv",
  "candidate_hash": "sha256:1a2b3c...",
  "assertions_file": "gold_set.jsonl",
  "assertions_hash": "sha256:f1e2d3...",
  "key_column": "comp_id",
  "input_verification": null,
  "summary": {
    "total": 216,
    "passed": 214,
    "failed": 1,
    "skipped": 1,
    "resolved": 215,
    "accuracy": 0.995,
    "coverage": 0.995,
    "by_severity": {
      "critical": { "passed": 40, "failed": 0, "skipped": 0 },
      "major": { "passed": 150, "failed": 1, "skipped": 1 },
      "minor": { "passed": 24, "failed": 0, "skipped": 0 }
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

| Field | Meaning |
|-------|---------|
| `version` | Report schema version (`benchmark.v0`) |
| `outcome` | `PASS`, `FAIL`, or `REFUSAL` |
| `candidate` | Candidate artifact label/path rendered in the report |
| `candidate_hash` | Content hash of the candidate artifact |
| `assertions_file` | Assertions artifact label/path |
| `assertions_hash` | Content hash of the assertions file |
| `key_column` | Candidate column used for entity lookup |
| `input_verification` | Lock verification result object or `null` when `--lock` absent |
| `summary` | Aggregate counts and score metrics |
| `failures` | Row-level failed assertions only |
| `skipped` | Row-level skipped assertions only |
| `refusal` | Refusal object for exit `2`; otherwise `null` |

### Output (human)

Default stdout should be a compact operator summary:

```text
BENCHMARK FAIL
candidate: normalized.csv
assertions: gold_set.jsonl
key: comp_id
passed: 214
failed: 1
skipped: 1
accuracy: 0.995
coverage: 0.995

FAIL comp_3 u8:adj_location expected=5.0% actual=5.5% compare_as=percent tolerance=0.01
SKIP comp_7 u8:cap_rate reason=SKIP_ENTITY
```

Human mode is a rendering of the same report contract, not a separate semantics path.

---

## Refusal codes

### Internal error taxonomy

`benchmark` should keep internal errors explicit and map them deterministically to refusal codes:

| Internal error variant | Maps to | Notes |
|------------------------|---------|-------|
| `BenchmarkError::CandidateIo` | `E_IO` | Candidate file unreadable |
| `BenchmarkError::AssertionsIo` | `E_IO` | Assertions file unreadable |
| `BenchmarkError::AssertionParse` | `E_BAD_ASSERTIONS` | Includes bad JSONL line and bad field typing |
| `BenchmarkError::AssertionSemantic` | `E_BAD_ASSERTIONS` | Illegal `compare_as` or tolerance usage |
| `BenchmarkError::EmptyAssertions` | `E_EMPTY_ASSERTIONS` | Zero valid assertions after parse |
| `BenchmarkError::FormatDetect` | `E_FORMAT_DETECT` | Unsupported or unreadable candidate format |
| `BenchmarkError::CandidateShape` | `E_FORMAT_DETECT` | Candidate is not one row-oriented relation |
| `BenchmarkError::KeyNotFound` | `E_KEY_NOT_FOUND` | `--key` missing from candidate |
| `BenchmarkError::KeyNotUnique` | `E_KEY_NOT_UNIQUE` | Duplicate key values for benchmarked rows |
| `BenchmarkError::KeyNull` | `E_KEY_NULL` | Null/blank key values for benchmarked rows |
| `BenchmarkError::InputNotLocked` | `E_INPUT_NOT_LOCKED` | Candidate absent from provided lockfiles |
| `BenchmarkError::InputDrift` | `E_INPUT_DRIFT` | Candidate hash mismatch against lock member |

| Code | Trigger | Next step |
|------|---------|-----------|
| `E_IO` | Can't read candidate or assertions file | Check paths |
| `E_BAD_ASSERTIONS` | Assertions file has unparseable lines | Fix assertions JSONL |
| `E_KEY_NOT_FOUND` | `--key` column not found in candidate | Check column name |
| `E_KEY_NOT_UNIQUE` | Candidate key column contains duplicates or ambiguous entity matches | Canonicalize first or choose a stable unique key |
| `E_KEY_NULL` | Candidate key column contains null/blank values for benchmarked rows | Clean the key column or canonicalize first |
| `E_FORMAT_DETECT` | Can't detect format of candidate file | Use a supported extension |
| `E_EMPTY_ASSERTIONS` | Assertions file has zero valid assertions | Add assertions |
| `E_INPUT_NOT_LOCKED` | Candidate not present in any provided lockfile | Re-run with correct `--lock` |
| `E_INPUT_DRIFT` | Candidate hash doesn't match lock member | Use the locked file |

### Refusal JSON envelope

```json
{
  "version": "benchmark.v0",
  "outcome": "REFUSAL",
  "refusal": {
    "code": "E_KEY_NOT_UNIQUE",
    "message": "Candidate key column 'comp_id' contains duplicate values",
    "detail": {
      "key_column": "comp_id",
      "sample_entities": ["comp_4", "comp_19"]
    },
    "next_command": "benchmark normalized.fixed.csv --assertions gold_set.jsonl --key comp_id --json"
  }
}
```

Refusal envelopes should be emitted for exit `2` in both default and `--json` modes. Human mode may pretty-print them, but the JSON contract must remain the system of record.

## Contract table

| Contract | Requirement | Enforced by |
|----------|-------------|-------------|
| `C01` | Candidate must materialize to one row-oriented relation | `candidate.rs` |
| `C02` | Assertions must parse line-by-line into valid `benchmark.v0` assertions | `assertions.rs` |
| `C03` | Candidate key column must exist before scoring starts | `key_check.rs` |
| `C04` | Candidate key values used for benchmark lookup must be unique | `key_check.rs` |
| `C05` | Candidate key values used for benchmark lookup must be non-null/non-blank | `key_check.rs` |
| `C06` | Field lookup uses exact canonical column names only | `engine.rs` |
| `C07` | Comparison mode semantics are explicit and deterministic | `compare.rs` |
| `C08` | Missing entity/field yields skip, not failure | `engine.rs` / `report.rs` |
| `C09` | Summary metrics separate correctness from coverage | `report.rs` |
| `C10` | Optional lock verification gates scoring input integrity | `lock_check.rs` |
| `C11` | Report ordering is stable for identical inputs | `engine.rs` / `report.rs` |
| `C12` | Human and JSON modes render the same underlying report data | `render.rs` |

## Threat / edge table

| Threat | Scenario | Required behavior |
|--------|----------|-------------------|
| `T01` | Assertions file contains malformed JSONL line | Refuse with `E_BAD_ASSERTIONS` and identify the bad line |
| `T02` | `--key` column missing from candidate | Refuse with `E_KEY_NOT_FOUND` |
| `T03` | Duplicate candidate rows match the same entity | Refuse with `E_KEY_NOT_UNIQUE` |
| `T04` | Candidate key contains null/blank values | Refuse with `E_KEY_NULL` |
| `T05` | Assertion declares illegal `compare_as` / `tolerance` combination | Refuse with `E_BAD_ASSERTIONS` |
| `T06` | Candidate format is nested/document-shaped JSON | Refuse with `E_FORMAT_DETECT` or a contract-equivalent refusal, never guess a flattening |
| `T07` | Every assertion skips because entities or fields are absent | Return a domain failure report with `accuracy = null`, not a refusal |
| `T08` | Candidate bytes do not match supplied lockfile | Refuse with `E_INPUT_DRIFT` before scoring |
| `T09` | Candidate file is not a member of supplied lockfile | Refuse with `E_INPUT_NOT_LOCKED` before scoring |
| `T10` | Assertion order differs between files with the same facts | Preserve input assertion order in row-level failures/skips so report diffing stays deterministic |

---

## Usage examples

```bash
# Score extraction against benchmark assertions
benchmark normalized.csv --assertions gold_set.jsonl --key comp_id --json

# Score a row-oriented JSONL extraction
benchmark normalized.jsonl --assertions lease_gold.jsonl --key tenant_id --json

# benchmark -> assess: raw metrics feed decision
benchmark normalized.csv --assertions gold.jsonl --key comp_id --json > benchmark.report.json
assess benchmark.report.json --policy extraction_quality.v1 > decision.json

# benchmark -> pack: seal the score as evidence
benchmark normalized.csv --assertions gold.jsonl --key comp_id --json > benchmark.report.json
pack seal benchmark.report.json normalized.csv gold.jsonl --output evidence/scored/
```

---

## Gold set flywheel

Each document processed, a human spot-checks N values. Those N assertions go into the gold set. Next time you reprocess, benchmark checks against all accumulated assertions. The gold set grows from 20 assertions (one document) to thousands (hundreds of documents). The gold set IS the factory's quality oracle.

## Tournament integration

Run N pipeline configurations, produce N packs, benchmark each against the same gold set, then rank by `summary.accuracy` with `summary.coverage` as a tie-breaker. The gold set is the constant; the pipeline configuration is the variable.

`summary.accuracy` measures correctness on resolved assertions only:

- `resolved = passed + failed`
- `accuracy = passed / resolved`
- `coverage = resolved / total`

This keeps wrong values and missing values separate instead of blending them into one number.

---

## `benchmark harvest`

Deferred past v0.

Fresh-eyes correction: approved outputs are not the same thing as gold truth. A pack that cleared `assess` is acceptable evidence, but it is not automatically a human-validated benchmark oracle. Promoting approved outputs directly into the gold set would let pipeline mistakes harden into future truth.

If a harvest helper is added later, it should be framed as **draft assertion generation** only:

- emit candidate assertions with provenance
- never auto-merge into the gold set
- require human review before any harvested assertion becomes benchmark truth
- stay outside the core `benchmark` scoring implementation

That keeps the gold set boundary intact: `benchmark` consumes ground truth; it does not mint it from prior outputs.

---

## Implementation notes

Rust binary with `duckdb-rs`. Loads the candidate and assertion JSONL into DuckDB tables, validates the key once, evaluates assertions via deterministic joins/projection queries, then constructs the report from the joined result set. This avoids N per-assertion round-trips and keeps score construction separate from lookup mechanics.

### Candidate crates

| Need | Crate | Notes |
|------|-------|-------|
| Embedded SQL engine | `duckdb` (bundled) | Format-agnostic candidate loading (same as `verify cross`) |
| JSON line parsing | `serde_json` | Line-by-line assertion JSONL parsing |
| Decimal/date comparison helpers | custom small module | Implements `string` / `number` / `percent` / `date` semantics explicitly |
| Content hashing | `sha2` | Assertions hash, candidate hash, input verification |

### Supported candidate formats (via DuckDB)

| Format | Auto-detected by |
|--------|------------------|
| CSV | `.csv` extension |
| Parquet | `.parquet` extension |
| JSON | `.json` extension, row-oriented only |
| JSONL | `.jsonl` extension |

Total implementation estimate: ~400-700 LOC of Rust.

---

## Determinism

Same candidate + same assertions = same report. `benchmark` should use deterministic load/join/projection patterns and include the candidate and assertion hashes in the report.

## Staged implementation sequence

### Stage D1 — CLI and refusal envelope

Build `cli.rs`, `refusal.rs`, and `main.rs` shell behavior.

- Supports: CLI contract and refusal-envelope wiring
- Must prove: `--help`, `--version`, `--json`, and refusal exit codes are stable before any DuckDB work starts

### Stage D2 — Assertion parser and schema checks

Build `assertions.rs` with line-by-line parsing, required field validation, `compare_as` parsing, tolerance validation, and stable assertion ordering.

- Satisfies: `C02`, `C07`
- Threats: `T01`, `T05`

### Stage D3 — Candidate loader

Build `candidate.rs` for extension-based format detection and DuckDB loading of the single row-oriented relation.

- Satisfies: `C01`
- Threats: `T06`

### Stage D4 — Key validation

Build `key_check.rs` to verify key existence, uniqueness, and non-null/non-blank behavior before scoring.

- Satisfies: `C03`, `C04`, `C05`
- Threats: `T02`, `T03`, `T04`

### Stage D5 — Comparison engine

Build `compare.rs` for explicit `string` / `number` / `percent` / `date` semantics.

- Satisfies: `C07`
- Must prove: tolerance semantics never leak into `string` or `date`

### Stage D6 — Score engine and report math

Build `engine.rs` and `report.rs` to join assertions to candidate rows, emit PASS/FAIL/SKIP outcomes, and compute summaries.

- Satisfies: `C06`, `C08`, `C09`, `C11`
- Threats: `T07`, `T10`

### Stage D7 — Lock integration

Build `lock_check.rs` and wire `--lock` so candidate membership/hash checks gate scoring.

- Satisfies: `C10`
- Threats: `T08`, `T09`

### Stage D8 — Human/JSON rendering

Build `render.rs` for the default operator summary and the JSON report shape.

- Satisfies: `C12`
- Must prove: human mode is a rendering of the same report contract, not a separate code path with separate math

### Stage D9 — Hardening and fixtures

Freeze fixtures, refusal examples, deterministic ordering tests, and representative candidate-format coverage.

- Satisfies: `I01` through `I12`
- Exit condition: all quality gates pass

## Test matrix

| Test ID | Covers | Type | Expected result |
|---------|--------|------|-----------------|
| `BENCH-U001` | `C02`, `T01` | unit | malformed assertion line refuses with `E_BAD_ASSERTIONS` |
| `BENCH-U002` | `C07`, `T05` | unit | illegal `compare_as` / `tolerance` combination refuses |
| `BENCH-U003` | `C07` | unit | `number` comparison honors absolute tolerance |
| `BENCH-U004` | `C07` | unit | `percent` comparison does not auto-convert ratio-form decimals |
| `BENCH-U005` | `C07` | unit | `date` comparison normalizes canonical date text |
| `BENCH-I001` | `C01`, `T06` | integration | nested/document-shaped JSON is rejected |
| `BENCH-I002` | `C03`, `T02` | integration | missing key column refuses with `E_KEY_NOT_FOUND` |
| `BENCH-I003` | `C04`, `T03` | integration | duplicate key rows refuse with `E_KEY_NOT_UNIQUE` |
| `BENCH-I004` | `C05`, `T04` | integration | null/blank key values refuse with `E_KEY_NULL` |
| `BENCH-I005` | `C06`, `C08` | integration | missing entity produces `SKIP_ENTITY` and does not increment failures |
| `BENCH-I006` | `C06`, `C08` | integration | missing field produces `SKIP_FIELD` and does not increment failures |
| `BENCH-I007` | `C09`, `T07` | integration | all-skipped report returns exit 1 with `accuracy = null` and correct coverage |
| `BENCH-I008` | `C09` | integration | mixed pass/fail/skip report computes `resolved`, `accuracy`, and `coverage` correctly |
| `BENCH-I009` | `C10`, `T08` | integration | hash drift against lockfile refuses before scoring |
| `BENCH-I010` | `C10`, `T09` | integration | non-member candidate against lockfile refuses before scoring |
| `BENCH-I011` | `C11`, `T10` | integration | identical inputs emit stable row ordering across repeated runs |
| `BENCH-I012` | `C12` | integration | human mode and JSON mode reflect the same failure and skip counts |

## Quality gates

- `Gate 1: Input contract gate` — malformed assertions, bad key discipline, and unsupported candidate shapes all refuse with the documented codes.
- `Gate 2: Comparison semantics gate` — `string`, `number`, `percent`, and `date` tests prove mode-specific behavior with no implicit fallback.
- `Gate 3: Summary math gate` — mixed and all-skipped fixtures prove `resolved`, `accuracy`, and `coverage` exactly.
- `Gate 4: Integrity gate` — `--lock` tests prove candidate membership and hash drift are checked before scoring.
- `Gate 5: Determinism gate` — repeated runs over the same fixture bytes produce byte-identical JSON output.
- `Gate 6: Rendering parity gate` — human output and JSON output disagree only in formatting, never in counts or row identities.

## Execution commands

Once the crate exists, the minimum implementation gate should be:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
ubs .
```

During active implementation, these focused commands should exist and stay green:

```bash
cargo test BENCH_U
cargo test BENCH_I
cargo test --test cli
cargo test --test scoring_matrix
cargo test --test refusals
cargo test --test lock_integration
```
