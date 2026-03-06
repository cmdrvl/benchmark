# benchmark — Extraction Accuracy Scoring

## One-line promise
**Check whether a dataset satisfies a set of expected (entity, field, value) assertions — scoring extraction quality against a human-validated reference.**

---

## Problem

You extract data from PDFs, Excel files, or SEC filings. How do you know the extraction is correct? You can't `rvl` the output against the reference — they're different shapes. The reference is a human's notes, a spot-check spreadsheet, a set of facts scattered across documents.

`benchmark` solves the **cross-shape comparison problem**. The reference isn't a dataset. It's a set of claims about what values should exist. Each assertion is schema-independent — `(comp_4, cap_rate, 6.76%)` doesn't care about column order, CSV structure, or file format. The assertion just needs to locate the value in the candidate and check it.

---

## Non-goals

`benchmark` is NOT:
- A diff tool (that's `rvl` or `compare`)
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

`0` PASS (all assertions satisfied) | `1` FAIL (one or more assertions failed) | `2` refusal

---

## Assertion file format (JSONL)

Each line is an independent assertion — an expected (entity, field, value) tuple:

```jsonl
{"entity": "comp_4", "field": "u8:cap_rate", "expected": "6.76%", "tolerance": "0.01", "source": "reference_excel:D14"}
{"entity": "comp_4", "field": "u8:sale_price", "expected": "28200000", "source": "reference_excel:D5"}
{"entity": "comp_1", "field": "u8:property_name", "expected": "Marquis at Briarcliff", "source": "reference_excel:B3"}
```

| Field | Required | Description |
|-------|----------|-------------|
| `entity` | yes | Row key value — matched against `--key` column in candidate |
| `field` | yes | Column name (canonical ID after canon) |
| `expected` | yes | Expected value (string representation) |
| `tolerance` | no | Numeric tolerance for approximate matching (absolute). Default: exact string match |
| `source` | no | Provenance of the benchmark fact (where the human found this value) |

---

## Lookup mechanics

For each assertion, benchmark queries the candidate via DuckDB: find the row where `key_column = entity`, read the value at `field`, compare to `expected`. Format auto-detected from candidate extension (same as `verify cross`).

### Value comparison

- If `tolerance` is absent: exact string match (after whitespace trimming)
- If `tolerance` is present: parse both values as numeric, check `abs(actual - expected) <= tolerance`
- If either value can't be parsed as numeric when `tolerance` is set: string comparison fallback

### Assertion outcomes

| Outcome | Meaning |
|---------|---------|
| `PASS` | Value matches (exact or within tolerance) |
| `FAIL` | Value doesn't match |
| `SKIP_ENTITY` | Entity key not found in candidate |
| `SKIP_FIELD` | Field not found in candidate |

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
    "accuracy": 0.991
  },
  "failures": [
    {
      "entity": "comp_3",
      "field": "u8:adj_location",
      "expected": "5.0%",
      "actual": "5.5%",
      "tolerance": "0.01",
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
| `E_FORMAT_DETECT` | Can't detect format of candidate file | Use a supported extension |
| `E_EMPTY_ASSERTIONS` | Assertions file has zero valid assertions | Add assertions |
| `E_INPUT_NOT_LOCKED` | Candidate not present in any provided lockfile | Re-run with correct `--lock` |
| `E_INPUT_DRIFT` | Candidate hash doesn't match lock member | Use the locked file |

---

## Usage examples

```bash
# Score extraction against benchmark assertions
benchmark normalized.csv --assertions gold_set.jsonl --key comp_id --json

# Score a JSON extraction (lease abstract from LLM)
benchmark lease_abstract.json --assertions lease_gold.jsonl --key tenant_id --json

# benchmark -> assess: accuracy score feeds decision
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

Run N pipeline configurations, produce N packs, benchmark each against the same gold set, rank by accuracy. The gold set is the constant; the pipeline configuration is the variable. Config A scores 214/216, Config B scores 210/216 — Config A wins. `assess` policies can set thresholds: "PROCEED if accuracy >= 0.95, ESCALATE if between 0.90 and 0.95, BLOCK if below 0.90."

---

## Implementation notes

Rust binary with `duckdb-rs`. Loads candidate as DuckDB table. Iterates assertions, executing a lookup query per assertion. Collects results. Reports aggregate accuracy.

### Candidate crates

| Need | Crate | Notes |
|------|-------|-------|
| Embedded SQL engine | `duckdb` (bundled) | Format-agnostic candidate loading (same as `verify cross`) |
| JSON line parsing | `serde_json` | Line-by-line assertion JSONL parsing |
| Content hashing | `sha2` | Assertions hash, candidate hash, input verification |

### Supported candidate formats (via DuckDB)

| Format | Auto-detected by |
|--------|------------------|
| CSV | `.csv` extension |
| Parquet | `.parquet` extension |
| JSON | `.json` extension |
| JSONL | `.jsonl` extension |

Total implementation estimate: ~300-500 LOC of Rust.

---

## Determinism

Same candidate + same assertions = same report. DuckDB is deterministic. The assertion file is content-hashed and included in the report.
