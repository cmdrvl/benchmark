# Expected Outputs

Golden JSON report templates keyed by fixture set name. These templates describe the expected
report structure and values for determinism gate validation.

Current expected outputs:

- `bench_mixed_fail.json` — mixed pass/fail/skip scenario (7 assertions, 4 pass, 1 fail, 2 skip)
- `bench_all_skip.json` — all entities absent (accuracy=null, coverage=0.0)

These templates also pin the top-level `tool` field and derived `policy_signals` surface so
determinism tests catch report-contract drift, not just score math drift.

Note: `candidate_hash` and `assertions_hash` fields are omitted from templates because
they depend on file content bytes. Determinism tests should validate structural
equivalence (counts, outcome, failure/skip records) rather than exact hash values.
