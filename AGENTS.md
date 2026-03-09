# AGENTS.md — benchmark

> Repo-specific guidance for AI coding agents working in `benchmark`.

This file adds repo-specific instructions on top of the shared monorepo rules when you are working inside the full `cmdrvl` workspace. In the standalone `benchmark` repo, treat this file and [docs/PLAN_BENCHMARK.md](./docs/PLAN_BENCHMARK.md) as the local source of truth.

---

## benchmark — What This Project Does

`benchmark` is the epistemic spine's **gold-set scoring primitive**.

It evaluates one row-oriented candidate relation against a human-validated assertion set and emits a deterministic score report.

Pipeline position:

```text
normalize / extract -> benchmark -> assess -> pack
```

What `benchmark` owns:

- assertion parsing
- candidate loading
- key validation
- explicit value comparison
- deterministic score construction
- deterministic derived quality signaling for downstream policy
- optional lock verification

What `benchmark` does not own:

- entity resolution (`canon`)
- structural comparability (`shape`)
- business-rule validation (`verify`)
- policy decisions (`assess`)
- gold-set generation from approved outputs

---

## Current Repository State

This repo now contains the implemented `benchmark v0` crate plus its fixture and quality-gate corpus.

Current contents:

- [docs/PLAN_BENCHMARK.md](./docs/PLAN_BENCHMARK.md) — full implementation-grade spec
- [.beads/issues.jsonl](./.beads/issues.jsonl) — execution graph for the implementation swarm
- [README.md](./README.md) — operator-facing contract and project framing
- Rust crate implementing CLI orchestration, parsing, loading, scoring, rendering, and lock verification
- fixture, expected-output, and perf-smoke coverage under `tests/fixtures/`

Implication:

- keep the implementation aligned to the plan
- do not collapse real module boundaries into `main.rs`
- do not add behavior that is merely "reasonable" if the plan does not say to do it

---

## Quick Reference

```bash
# Read the spec first
sed -n '1,260p' docs/PLAN_BENCHMARK.md

# See the execution graph
br ready
br blocked

# AI-agent prioritization
bv -robot-next
bv -robot-triage -robot-max-results 5
bv -robot-plan

# Current crate verification
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
ubs .

# Mandatory gate once the crate exists
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
ubs .
./scripts/ubs_gate.sh
```

---

## Source of Truth

- **Spec:** [docs/PLAN_BENCHMARK.md](./docs/PLAN_BENCHMARK.md)
- **Execution graph:** [.beads/issues.jsonl](./.beads/issues.jsonl)

If code, README, and plan disagree, the plan wins.

Do not invent behavior not present in the plan.

---

## Implemented File Map

The current implementation structure is:

| File | Purpose |
|------|---------|
| `src/lib.rs` | module root and shared library surface |
| `src/main.rs` | thin CLI entrypoint only |
| `src/cli.rs` | clap parsing, flag semantics, mode wiring |
| `src/assertions.rs` | JSONL parsing, semantic validation, stable ordering |
| `src/candidate.rs` | format detection and DuckDB relation loading |
| `src/key_check.rs` | key existence, uniqueness, and null-key validation |
| `src/compare.rs` | `string` / `number` / `percent` / `date` semantics |
| `src/engine.rs` | assertion evaluation and outcome generation |
| `src/lock_check.rs` | optional lock membership and hash verification |
| `src/report.rs` | internal report model and summary math |
| `src/render.rs` | human and JSON report rendering |
| `src/refusal.rs` | refusal taxonomy and refusal envelopes |
| `tests/fixtures/` | shared candidate, assertion, lock, and perf fixtures |
| `tests/cli.rs` | CLI mode and exit-code tests |
| `tests/scoring_matrix.rs` | comparison and scoring tests |
| `tests/refusals.rs` | refusal-path tests |
| `tests/lock_integration.rs` | lock verification tests |
| `tests/perf_smoke.rs` or `benches/` | runtime smoke guardrails |

Critical structural rule:

- `src/main.rs` stays thin
- module declarations and shared APIs belong in `src/lib.rs`

---

## Output Contract (Critical)

Target domain outcomes:

| Exit | Outcome | Meaning |
|------|---------|---------|
| `0` | `PASS` | all assertions satisfied, no skips |
| `1` | `FAIL` | one or more assertions failed or skipped |
| `2` | `REFUSAL` | benchmark could not score safely |

Target output modes:

- default stdout: compact human-readable report
- `--json`: machine-readable full report
- stderr: process diagnostics only

Refusal envelopes are part of the contract. Do not replace them with ad-hoc text.

Machine-report specifics that matter for downstream compatibility:

- scoring reports and refusals emit top-level `tool: "benchmark"`
- scoring reports emit derived `policy_signals.quality_band`
- refusal envelopes emit stable top-level `policy_signals: {}`
- `quality_band` is derived from the raw summary; it is not a separate decision engine

---

## Core Invariants (Do Not Break)

### 1. One relation only

`benchmark v0` evaluates exactly one row-oriented relation at a time.

- no document-shaped JSON flattening
- no multi-relation join semantics
- no heuristic schema inference

### 2. Key discipline is mandatory

The benchmark key must:

- exist
- be unique for benchmarked rows
- contain no null or blank values for benchmarked rows

Duplicate or null key values are refusals, not warnings.

### 3. Comparison semantics are explicit

Every assertion runs under one declared or default `compare_as` mode.

- no implicit numeric/date guessing
- no numeric parse failure fallback to string equality
- tolerance is only legal for `number` and `percent`

### 4. Missingness is not failure

Missing entities and missing fields produce skips only.

Do not:

- convert skips into failures
- convert skips into passes
- fabricate missing values

### 5. Accuracy and coverage stay separate

Hard math invariants:

- `resolved = passed + failed`
- `skipped = total - resolved`
- `accuracy = passed / resolved`
- `coverage = resolved / total`

If `resolved = 0`, `accuracy` must be `null`.

### 6. Policy signals are derived only

`benchmark` may expose a coarse `quality_band` for downstream `assess` policies.

That signal must:

- be a pure function of the benchmark summary
- stay auditable against `failed`, `skipped`, `accuracy`, and `coverage`
- never replace the raw metrics as the source of truth
- never make proceed/block decisions inside `benchmark`

### 7. Ground truth is imported, not minted

Approved outputs are not automatically gold truth.

Do not implement any shortcut that promotes prior pipeline outputs into benchmark assertions without explicit human review.

### 8. Deterministic ordering

Failures and skips must preserve stable assertion order.

Same candidate bytes plus same assertions bytes should produce the same ordered report.

### 9. Candidate load should not thrash

Target runtime safety rule:

- load the candidate relation once per run
- reuse that loaded relation across key checks and scoring

Do not accidentally create an O(assertions) reload loop.

---

## Toolchain

Target implementation assumptions:

- language: Rust
- package manager: Cargo only
- edition: 2024
- unsafe code: forbidden

Expected core dependencies:

- `clap` for CLI parsing
- `serde` and `serde_json` for structured IO
- `duckdb` for candidate loading and evaluation
- `sha2` for content hashing

Do not add heavy dependencies casually. This tool should stay small and deterministic.

---

## Quality Gates

### Docs-only changes

Run this after doc-only changes:

```bash
git diff --check
ubs .
```

### Routine code changes

Run this after substantive code changes:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
ubs .
```

### Runtime or determinism-sensitive changes

Add this when changing the end-to-end scoring path, determinism behavior, or runtime-sensitive code:

```bash
cargo test --test perf_smoke -- --nocapture
```

### Stop-ship verification target

Before calling the tool implementation done, the quality-gate bead should prove:

- named `BENCH_U` and `BENCH_I` coverage exists
- repeated JSON runs over the same fixture bytes are byte-identical
- perf smoke guardrails run without changing correctness semantics

---

## MCP Agent Mail — Multi-Agent Coordination

Agent Mail is the coordination layer for multi-agent sessions in this repo: identities, inbox/outbox, thread history, and advisory file reservations.

### Session Baseline

1. If direct MCP Agent Mail tools are available in this harness, ensure project and reuse your identity:
   - `ensure_project(project_key=<abs-path>)`
   - `whois(project_key, agent_name)` or `register_agent(...)` only if identity does not exist
2. Reserve only exact files you will edit:
   - Allowed: `src/engine.rs`, `tests/cli.rs`
   - Not allowed: `src/**`, `tests/**`, whole directories
3. Send a short start message and finish message for each bead, reusing the bead ID as the thread when practical.
4. Check inbox at moderate cadence (roughly every 2-5 minutes), not continuously.

### Important `ntm` Boundary

When this repo is worked via `ntm`, the session may be connected to Agent Mail even if the spawned Codex or Claude harness does **not** expose direct `mcp__mcp-agent-mail__...` tools.

If direct MCP Agent Mail tools are unavailable:

- do **not** stop working just because mail tools are absent
- continue with `br`, exact file reservations via the available coordination surface, and overseer instructions
- treat Beads + narrow file ownership as the minimum coordination contract

### Stability Rules

- Do not run retry loops for `register_agent`, `create_agent_identity`, or `macro_start_session`.
- If a call fails with a transient DB/SQLite lock error, back off for 90 seconds before retrying.
- Continue bead work while waiting for retry windows; do not block all progress on mail retries.

### Communication Rules

- If a message has `ack_required=true`, acknowledge it promptly.
- Keep bead updates short and explicit: start message, finish message, blocker message.
- Reuse a stable bead thread when possible for searchable history.

### Reservation Rules

- Reserve only specific files you are actively editing.
- Never reserve entire directories or broad patterns.
- If a reservation conflict appears, pick another unblocked bead or a non-overlapping file.

---

## br (beads_rust) — Dependency-Aware Issue Tracking

**Note:** `br` is non-invasive and never executes git commands. After `br sync --flush-only`, you must manually run `git add .beads/` and `git commit`.

Beads is the execution source of truth in this repo.

- Beads = task graph, state, priorities, dependencies
- Agent Mail = coordination, reservations, audit trail

```bash
br ready
br show <id>
br update <id> --status in_progress
br close <id> --reason "Completed"
br sync --flush-only
git add .beads/
git commit -m "sync beads"
```

Conventions:

- include bead IDs in coordination subjects, for example `[<bead-id>] Start engine/report math`
- use the bead ID in reservation reasons when the tool supports it
- prefer concrete ready beads over the epic tracker

Workflow:

1. Start with `br ready`.
2. Mark the bead `in_progress` before editing.
3. Reserve exact files and send a short start update when coordination tools are available.
4. Implement and run the right quality gate.
5. Close the bead, send a completion note, and release reservations.

Repo-specific graph shape:

```text
foundation
  -> cli
  -> assertions
  -> candidate
  -> fixtures
  -> compare
  -> lock_check

candidate -> key_check
assertions + candidate + key_check + compare -> engine
engine -> render
cli + render + fixtures + lock_check + engine -> integration
integration -> perf_smoke -> quality_gates
```

Important:

- the epic is tracker noise; prefer concrete ready beads
- do not start blocked feature work just because the file seems obvious
- if a bead is `in_progress` with no assignee, no comments, and no active reservation, reopen it before using `bv` triage

Recommended AI-agent triage loop:

```bash
# Re-open clearly stale work first when needed
br list --status in_progress --pretty

# Then pick the next highest-value bead
bv -robot-next
bv -robot-triage -robot-max-results 5
```

---

## File Reservation Guidance

This repo is being optimized for parallel implementation. Reserve exact files only.

Per-lane target surfaces:

| Lane | Expected files |
|------|----------------|
| foundation | `Cargo.toml`, `src/lib.rs`, `src/main.rs` |
| cli | `src/cli.rs`, `src/refusal.rs`, `tests/cli.rs` |
| assertions | `src/assertions.rs`, assertion-focused tests |
| candidate | `src/candidate.rs`, candidate fixtures/tests |
| fixtures | `tests/fixtures/**`, test harness files |
| key_check | `src/key_check.rs`, key-validation tests |
| compare | `src/compare.rs`, comparison tests |
| engine | `src/engine.rs`, `src/report.rs`, scoring tests |
| render | `src/render.rs`, output-format tests |
| lock_check | `src/lock_check.rs`, lock tests |
| integration | `src/main.rs`, minimal end-to-end tests |
| perf | `tests/perf_smoke.rs` or `benches/`, perf fixtures |

Do not reserve broad globs like `src/**` or `tests/**`.

---

## Project-Specific Guidance

### Keep render separate from scoring

`src/report.rs` and `src/engine.rs` own report data and summary math.

`src/render.rs` must only format existing report data.

Do not re-derive counts in the renderer.

### Keep compare separate from assertions

`src/assertions.rs` owns parsing and semantic validation.

`src/compare.rs` owns actual comparison behavior.

Do not leak comparison policy into the parser.

### Keep lock verification separate from integration

`src/lock_check.rs` should expose a reusable library surface.

Do not bury lock semantics directly in `main.rs`.

### Prefer plan terms in code and tests

Use the plan vocabulary directly:

- `PASS`
- `FAIL`
- `SKIP_ENTITY`
- `SKIP_FIELD`
- `REFUSAL`
- `accuracy`
- `coverage`
- `quality_band`
- `quality_band_basis`
- `input_verification`

Avoid renaming these into "friendlier" local synonyms.

---

## CI / Release Status

Current repo reality:

- Rust crate exists and is locally runnable with `cargo run -- ...`
- CI workflow exists at `.github/workflows/ci.yml`
- release workflow exists at `.github/workflows/release.yml`
- Homebrew tap update is part of the release workflow
- no published binary yet

Do not add README badges or install claims until they are real.

Release discipline in this repo now follows the stronger spine pattern:

- `fmt` / `clippy` / `test` before publish
- `./scripts/ubs_gate.sh` in CI
- deterministic artifacts
- `main` as primary branch
- sync `master` for legacy compatibility

---

## Editing Rules

- do not widen v0 scope just because a broader architecture sounds cleaner
- do not hide plan disagreements in implementation details
- do not add automatic gold-set minting behavior
- do not make `main.rs` the real implementation layer
- do not optimize runtime by changing semantics
- do add tests or perf smoke checks when runtime work could change behavior

---

## Multi-Agent Notes

This repo is explicitly being prepared for parallel agent work.

That means:

1. Keep changes granular.
2. Prefer one or two file touches per bead.
3. Use existing module boundaries instead of introducing cross-cutting helpers early.
4. If a performance improvement risks semantics, add a regression guardrail in tests or perf smoke coverage.

---

## Session Completion

Before ending a session in this repo:

1. verify plan alignment with [docs/PLAN_BENCHMARK.md](./docs/PLAN_BENCHMARK.md)
2. run the right quality gate for the current repo state
3. sync Beads if you changed issue state
4. confirm any file reservations or bead comments reflect the actual handoff state
5. if you were explicitly asked to commit or push in this environment, do so with a precise message
6. confirm `git status` accurately reflects what remains uncommitted
