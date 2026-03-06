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

## `benchmark harvest` — Gold Set Generation from Approved Evidence

### One-line promise

**Turn approved data into future test cases — automatically growing the gold set from your evidence archive.**

### Problem

The gold set flywheel described above has a manual bottleneck: humans write JSONL assertions by hand. For a 200-row dataset with 15 columns, that's 3,000 potential assertions. Nobody writes 3,000 lines. They spot-check 20 values and stop. So the gold set stays thin, benchmark's coverage stays shallow, and regressions hide in untested cells.

`benchmark harvest` breaks the bottleneck. When data passes through the pipeline — `shape` says COMPATIBLE, `verify` says PASS, `assess` says PROCEED — and gets sealed into an evidence pack, that data has been epistemically validated. `harvest` extracts assertions from that trusted data, tagged with full provenance. The evidence archive becomes the gold set source.

The compound effect: each month you process data, approve it, and pack it. Next month, every approved value from every prior month tests the new extraction. Month 1: 200 assertions. Month 12: 2,400 assertions. Month 24: your extraction pipeline is tested against every value you've ever approved. Regressions become impossible to hide.

### CLI

```
benchmark harvest <SOURCE>... --key <COLUMN> [OPTIONS]

Arguments:
  <SOURCE>...            Trusted data files or evidence packs to harvest from

Options:
  --key <COLUMN>         Key column for entity identification
  --fields <COL>...      Only harvest these columns (default: all)
  --sample <N>           Harvest N random assertions per entity (deterministic seed)
  --seed <INT>           Seed for --sample (default: 0, for reproducibility)
  --tolerance <FILE>     YAML file mapping column names to default tolerances
  --merge <FILE>         Merge into existing assertion file (deduplicate + flag conflicts)
  --require-decision     Only harvest from packs whose assess decision is PROCEED
  --output <FILE>        Output file (default: stdout)
  --json                 JSON output (harvest report, not the assertions themselves)
```

`<SOURCE>` accepts raw data files (CSV, JSON, Parquet) or evidence pack directories/archives. When given a pack, `harvest` extracts the data files using the pack's lockfile for provenance verification.

### Modes

**File mode**: `benchmark harvest normalized.csv --key comp_id`
Harvest assertions directly from a trusted data file. The caller is asserting this file is correct.

**Pack mode**: `benchmark harvest evidence/2025-*/ --key comp_id --require-decision`
Harvest from evidence packs. For each pack: verify integrity, check for an `assess` decision (if `--require-decision`), extract the data file, generate assertions. Each assertion's provenance traces back to the pack.

### Output: assertion file (JSONL)

```jsonl
{"entity": "comp_4", "field": "u8:cap_rate", "expected": "6.76%", "source": "harvest:evidence/2025-11/pack-a7f3b2:normalized.csv:sha256:1a2b3c"}
{"entity": "comp_4", "field": "u8:sale_price", "expected": "28200000", "source": "harvest:evidence/2025-11/pack-a7f3b2:normalized.csv:sha256:1a2b3c"}
{"entity": "comp_1", "field": "u8:property_name", "expected": "Marquis at Briarcliff", "source": "harvest:evidence/2025-11/pack-a7f3b2:normalized.csv:sha256:1a2b3c"}
```

Every harvested assertion carries a `source` field with full provenance: `harvest:<pack_path>:<data_file>:<content_hash>`. You always know exactly where the expected value came from.

### Output: harvest report (with `--json`)

```json
{
  "version": "benchmark_harvest.v0",
  "sources_scanned": 12,
  "sources_harvested": 11,
  "sources_skipped": 1,
  "assertions_generated": 2847,
  "assertions_per_source": [
    { "source": "evidence/2025-01/pack-a1b2c3", "assertions": 237, "entities": 15, "fields": 16 },
    { "source": "evidence/2025-02/pack-d4e5f6", "assertions": 252, "entities": 16, "fields": 16 }
  ],
  "skipped": [
    { "source": "evidence/2025-03/pack-g7h8i9", "reason": "decision_not_proceed", "decision_band": "ESCALATE" }
  ],
  "conflicts": null
}
```

### `--merge`: growing the gold set

`benchmark harvest --merge gold.jsonl` reads the existing gold set, generates new assertions, and produces a merged file with three outcomes per assertion:

| Outcome | Meaning |
|---------|---------|
| `kept` | Existing assertion, no new value for this (entity, field) |
| `added` | New (entity, field) pair not in existing set |
| `conflict` | Same (entity, field) but different `expected` value |

Conflicts are **not silently resolved**. They're written to a separate conflicts file (`<output>.conflicts.jsonl`) for human review:

```jsonl
{"entity": "comp_4", "field": "u8:cap_rate", "existing": "6.76%", "existing_source": "harvest:evidence/2025-06/pack-x:normalized.csv:sha256:abc", "incoming": "6.80%", "incoming_source": "harvest:evidence/2025-12/pack-y:normalized.csv:sha256:def"}
```

A conflict means: two approved datasets disagree on the value for the same entity and field. This is a signal, not an error. Either the value legitimately changed between periods (in which case `rvl` should have flagged it), or one of the extractions was wrong. Either way, a human should decide which value is canonical.

The merge report (with `--json`) includes conflict counts:

```json
{
  "merge": {
    "existing_assertions": 1200,
    "incoming_assertions": 2847,
    "kept": 1180,
    "added": 1667,
    "conflicts": 20,
    "conflicts_file": "gold_set.conflicts.jsonl",
    "final_assertions": 2847
  }
}
```

### `--tolerance` file

A YAML file mapping column names to default tolerances, applied to all harvested assertions for those columns:

```yaml
# tolerance.yaml
u8:cap_rate: "0.005"
u8:sale_price: "1000"
u8:noi: "500"
u8:gba: "10"
```

Columns not in the tolerance file get exact string match (no tolerance). This lets you express "cap rates can wiggle by half a basis point across extractions" without per-assertion annotation.

### `--sample` mode

For large datasets, `--sample N` harvests N random assertions per entity instead of all cells. The seed is deterministic (default 0), so the same source + same N + same seed = same assertions. This is useful for keeping gold sets manageable while still growing coverage over time: `--sample 5` across 12 monthly packs with 200 entities each produces 12,000 assertions — comprehensive but not overwhelming.

### Refusal codes (harvest-specific)

| Code | Trigger | Next step |
|------|---------|-----------|
| `E_NO_SOURCES` | No valid sources found | Check paths/glob |
| `E_PACK_UNREADABLE` | Can't read or parse pack | Check pack integrity |
| `E_NO_DATA` | Pack contains no data files (only reports) | Check pack contents |
| `E_NO_DECISION` | `--require-decision` but pack has no assess decision | Remove flag or add decision to pack |
| `E_MERGE_IO` | Can't read existing file for `--merge` | Check merge file path |

Plus all standard `benchmark` refusal codes (`E_IO`, `E_KEY_NOT_FOUND`, etc.).

### Usage examples

```bash
# Harvest from a single trusted file
benchmark harvest normalized.csv --key comp_id --output gold.jsonl

# Harvest from all approved packs in 2025
benchmark harvest evidence/2025-*/ \
  --key comp_id --require-decision \
  --output gold.jsonl

# Harvest with tolerances for numeric columns
benchmark harvest evidence/2025-*/ \
  --key comp_id --require-decision \
  --tolerance tolerance.yaml \
  --output gold.jsonl

# Grow an existing gold set (merge + conflict detection)
benchmark harvest evidence/2025-12/ \
  --key comp_id --require-decision \
  --merge gold.jsonl --output gold.jsonl
# Review conflicts: gold.jsonl.conflicts.jsonl

# Sample mode: 5 assertions per entity, manageable size
benchmark harvest evidence/2025-*/ \
  --key comp_id --sample 5 --seed 42 \
  --output gold_sampled.jsonl

# Full pipeline: harvest → score → assess → pack
benchmark harvest evidence/2025-*/ --key comp_id --require-decision \
  --merge gold.jsonl --output gold.jsonl
benchmark normalized_new.csv --assertions gold.jsonl --key comp_id --json \
  > benchmark.report.json
assess benchmark.report.json --policy extraction_quality.v1 > decision.json
pack seal benchmark.report.json normalized_new.csv gold.jsonl decision.json \
  --output evidence/2026-01/
# Next month, harvest from this pack too — the circle closes
```

### The closed loop

```
approve data → pack → harvest → gold set → benchmark next extraction → approve → pack → harvest → ...
```

Each iteration:
1. More assertions accumulate (coverage grows)
2. Conflicts surface disagreements (quality improves)
3. Regressions get caught earlier (confidence increases)
4. The gold set converges on comprehensive ground truth

The gold set is never "done" — it's a living asset that grows with every approved extraction. `harvest` is the mechanism that makes the flywheel turn.

### Why this matters

1. **Breaks the manual bottleneck.** Gold set creation goes from hand-writing JSONL to running a command. Coverage jumps from 20 spot-checked values to thousands of assertions.
2. **Provenance is built in.** Every assertion traces to a specific pack, file, and content hash. You can audit why any expected value is what it is.
3. **Conflicts are signals.** When two approved datasets disagree on the same entity+field, that's not a merge failure — it's a data quality insight that would otherwise go unnoticed.
4. **Composes with everything.** Harvest reads from `pack`. Its output feeds `benchmark`. Benchmark's output feeds `assess`. Assess's output gets sealed into `pack`. The tools form a self-reinforcing loop.
5. **Deterministic.** Same sources + same options = same assertions. The harvest itself is reproducible and auditable.

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
