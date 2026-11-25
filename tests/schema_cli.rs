//! `--schema`: the checked-in JSON Schema must describe real benchmark.v0 output.

use std::process::{Command, Output};

use serde_json::{Value, json};

fn benchmark(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_benchmark"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .expect("benchmark should run")
}

fn schema() -> Value {
    let output = benchmark(&["--schema"]);
    assert_eq!(output.status.code(), Some(0));
    serde_json::from_slice(&output.stdout).expect("--schema is JSON")
}

fn run_json(candidate: &str, gold: &str) -> (Option<i32>, Value) {
    let output = benchmark(&[
        candidate,
        "--assertions",
        gold,
        "--key",
        "comp_id",
        "--json",
        "--no-witness",
    ]);
    let value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    (output.status.code(), value)
}

#[test]
fn schema_is_the_checked_in_document() {
    assert_eq!(
        schema(),
        serde_json::from_str::<Value>(include_str!("../benchmark.schema.json"))
            .expect("schema file is JSON")
    );
}

#[test]
fn real_pass_fail_and_refusal_outputs_validate() {
    let schema = schema();
    let validator = jsonschema::validator_for(&schema).expect("schema compiles");

    let cases = [
        (
            "PASS",
            run_json(
                "tests/fixtures/candidates/smoke/bench_i001_candidate.csv",
                "tests/fixtures/assertions/smoke/bench_i001_gold.jsonl",
            ),
        ),
        (
            "FAIL",
            run_json(
                "tests/fixtures/candidates/smoke/bench_mixed.csv",
                "tests/fixtures/assertions/smoke/bench_mixed_gold.jsonl",
            ),
        ),
        (
            "REFUSAL",
            run_json(
                "/nonexistent/candidate.csv",
                "tests/fixtures/assertions/smoke/bench_i001_gold.jsonl",
            ),
        ),
    ];
    for (label, (_, value)) in &cases {
        assert_eq!(value["outcome"], *label);
        assert!(
            validator.is_valid(value),
            "{label} output does not match --schema: {value}"
        );
    }
}

/// Planted negatives: documents that break the contract must not validate.
#[test]
fn schema_rejects_drifted_documents() {
    let schema = schema();
    let validator = jsonschema::validator_for(&schema).expect("schema compiles");
    let (_, good) = run_json(
        "tests/fixtures/candidates/smoke/bench_i001_candidate.csv",
        "tests/fixtures/assertions/smoke/bench_i001_gold.jsonl",
    );
    assert!(validator.is_valid(&good));

    let mut bad_outcome = good.clone();
    bad_outcome["outcome"] = json!("MAYBE");
    assert!(!validator.is_valid(&bad_outcome));

    let mut missing_summary = good.clone();
    missing_summary
        .as_object_mut()
        .expect("report is an object")
        .remove("summary");
    assert!(!validator.is_valid(&missing_summary));

    let mut wrong_version = good.clone();
    wrong_version["version"] = json!("benchmark.v1");
    assert!(!validator.is_valid(&wrong_version));

    let mut bad_code = json!({
        "tool": "benchmark", "version": "benchmark.v0", "outcome": "REFUSAL",
        "policy_signals": {},
        "refusal": {"code": "NOT_PREFIXED", "message": "m", "detail": {}, "next_command": null}
    });
    assert!(!validator.is_valid(&bad_code));
    bad_code["refusal"]["code"] = json!("E_IO");
    assert!(validator.is_valid(&bad_code));
}
