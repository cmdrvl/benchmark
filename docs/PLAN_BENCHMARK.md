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

## Candidate contract

`benchmark` scores one **row-oriented relation** at a time.

That means v0 candidates must materialize to a single table-like surface with:

- one stable key column named by `--key`
- scalar fields addressable by column name
- one row per entity

CSV, JSONL, Parquet, and row-shaped JSON are fine when DuckDB can materialize them this way. Nested JSON objects, arrays-of-arrays, or document-shaped blobs that do not expose a single scalar relation are out of scope for v0 and should be normalized before scoring.

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

---

## Refusal codes

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
