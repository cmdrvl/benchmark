use std::path::{Path, PathBuf};

use benchmark::{
    Outcome,
    assertions::Severity,
    cli::Cli,
    compare::CompareAs,
    execute,
    lock_check::InputVerification,
    render::render_report,
    report::{AssertionOutcome, BenchmarkReport, EvaluatedAssertion, ReportContext, ReportOutcome},
};
use clap::CommandFactory;

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn fixture_cli(candidate: &str, assertions: &str, lockfiles: &[&str], json: bool) -> Cli {
    Cli {
        candidate: fixture(candidate),
        assertions: fixture(assertions),
        key: "comp_id".to_owned(),
        lock: lockfiles.iter().map(|path| fixture(path)).collect(),
        json,
    }
}

fn sample_report(outcome: AssertionOutcome) -> BenchmarkReport {
    BenchmarkReport::from_evaluated(
        ReportContext {
            candidate: "normalized.csv".to_owned(),
            candidate_hash: "sha256:candidate".to_owned(),
            assertions_file: "gold.jsonl".to_owned(),
            assertions_hash: "sha256:assertions".to_owned(),
            key_column: "comp_id".to_owned(),
            input_verification: Some(InputVerification {
                lockfiles: vec![PathBuf::from("candidate.lock.json")],
                matched_lockfile: PathBuf::from("candidate.lock.json"),
                verified_member: "normalized.csv".to_owned(),
                candidate_hash: "sha256:candidate".to_owned(),
            }),
        },
        vec![EvaluatedAssertion {
            entity: "comp_1".to_owned(),
            field: "cap_rate".to_owned(),
            expected: "5.0%".to_owned(),
            actual: Some("5.5%".to_owned()),
            compare_as: CompareAs::Percent,
            tolerance: Some(0.01),
            severity: Severity::Major,
            source: Some("reference_excel:E18".to_owned()),
            outcome,
            detail: None,
        }],
    )
}

#[test]
fn bench_u_cli_contract_is_stable() {
    let command = Cli::command();
    command.debug_assert();
}

#[test]
fn bench_u_help_mentions_expected_flags() {
    let mut command = Cli::command();
    let help = command.render_long_help().to_string();

    assert!(help.contains("Usage: benchmark"));
    assert!(help.contains("<CANDIDATE>"));
    assert!(help.contains("--assertions"));
    assert!(help.contains("--key"));
    assert!(help.contains("--lock"));
    assert!(help.contains("--json"));
}

#[test]
fn bench_u_version_is_wired() {
    let version = Cli::command().render_version().to_string();
    assert!(version.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn bench_u_execute_returns_json_refusal_with_exit_2() -> Result<(), Box<dyn std::error::Error>> {
    let execution = execute(fixture_cli(
        "tests/fixtures/candidates/smoke/bench_i001_candidate.csv",
        "tests/fixtures/assertions/smoke/bench_i001_gold.jsonl",
        &["tests/fixtures/locks/refusal/bench_non_member.lock.json"],
        true,
    ))?;

    assert_eq!(execution.outcome, Outcome::Refusal);
    assert_eq!(execution.exit_code(), 2);

    let json: serde_json::Value = serde_json::from_str(&execution.stdout)?;
    assert_eq!(json["version"], "benchmark.v0");
    assert_eq!(json["outcome"], "REFUSAL");
    assert_eq!(json["refusal"]["code"], "E_INPUT_NOT_LOCKED");
    assert!(json["refusal"]["next_command"].as_str().is_some());

    Ok(())
}

#[test]
fn bench_u_execute_returns_human_refusal_with_exit_2() -> Result<(), Box<dyn std::error::Error>> {
    let execution = execute(fixture_cli(
        "tests/fixtures/candidates/smoke/bench_i001_candidate.csv",
        "tests/fixtures/assertions/refusal/bench_u001_malformed.jsonl",
        &[],
        false,
    ))?;

    assert_eq!(execution.outcome, Outcome::Refusal);
    assert_eq!(execution.exit_code(), 2);
    assert!(execution.stdout.contains("REFUSAL [E_BAD_ASSERTIONS]"));
    assert!(execution.stdout.contains("next: benchmark "));

    Ok(())
}

#[test]
fn bench_i_execute_returns_json_pass_with_exit_0() -> Result<(), Box<dyn std::error::Error>> {
    let execution = execute(fixture_cli(
        "tests/fixtures/candidates/smoke/bench_i001_candidate.csv",
        "tests/fixtures/assertions/smoke/bench_i001_gold.jsonl",
        &["tests/fixtures/locks/smoke/bench_i010_candidate.lock.json"],
        true,
    ))?;

    assert_eq!(execution.outcome, Outcome::Pass);
    assert_eq!(execution.exit_code(), 0);

    let json: serde_json::Value = serde_json::from_str(&execution.stdout)?;
    assert_eq!(json["outcome"], "PASS");
    assert_eq!(json["summary"]["passed"], 2);
    assert_eq!(json["summary"]["failed"], 0);
    assert_eq!(json["summary"]["skipped"], 0);
    assert_eq!(
        json["input_verification"]["verified_member"],
        "bench_i001_candidate.csv"
    );
    assert_eq!(json["refusal"], serde_json::Value::Null);

    Ok(())
}

#[test]
fn bench_i_execute_returns_human_fail_with_exit_1() -> Result<(), Box<dyn std::error::Error>> {
    let execution = execute(fixture_cli(
        "tests/fixtures/candidates/smoke/bench_mixed.csv",
        "tests/fixtures/assertions/smoke/bench_mixed_gold.jsonl",
        &[],
        false,
    ))?;

    assert_eq!(execution.outcome, Outcome::Fail);
    assert_eq!(execution.exit_code(), 1);
    assert!(execution.stdout.contains("BENCHMARK FAIL"));
    assert!(execution.stdout.contains("passed: 4"));
    assert!(execution.stdout.contains("failed: 1"));
    assert!(execution.stdout.contains("skipped: 2"));
    assert!(execution.stdout.contains(
        "FAIL comp_3 cap_rate expected=7.50% actual=7.25% compare_as=percent tolerance=0.01"
    ));
    assert!(
        execution
            .stdout
            .contains("SKIP comp_7 cap_rate reason=SKIP_ENTITY")
    );
    assert!(
        execution
            .stdout
            .contains("SKIP comp_4 nonexistent_field reason=SKIP_FIELD")
    );

    Ok(())
}

#[test]
fn bench_u_render_report_json_preserves_machine_contract() -> Result<(), Box<dyn std::error::Error>>
{
    let rendered = render_report(&sample_report(AssertionOutcome::Fail), true)?;
    let json: serde_json::Value = serde_json::from_str(&rendered)?;

    assert_eq!(json["version"], "benchmark.v0");
    assert_eq!(json["outcome"], "FAIL");
    assert_eq!(json["candidate"], "normalized.csv");
    assert_eq!(json["candidate_hash"], "sha256:candidate");
    assert_eq!(json["assertions_hash"], "sha256:assertions");
    assert_eq!(json["summary"]["failed"], 1);
    assert_eq!(json["summary"]["by_severity"]["major"]["failed"], 1);
    assert_eq!(
        json["input_verification"]["verified_member"],
        "normalized.csv"
    );
    assert_eq!(json["failures"][0]["compare_as"], "percent");
    assert_eq!(json["refusal"], serde_json::Value::Null);

    Ok(())
}

#[test]
fn bench_u_render_report_human_renders_compact_fail_summary()
-> Result<(), Box<dyn std::error::Error>> {
    let rendered = render_report(&sample_report(AssertionOutcome::Fail), false)?;

    assert!(rendered.contains("BENCHMARK FAIL"));
    assert!(rendered.contains("candidate: normalized.csv"));
    assert!(rendered.contains("assertions: gold.jsonl"));
    assert!(rendered.contains("key: comp_id"));
    assert!(rendered.contains("passed: 0"));
    assert!(rendered.contains("failed: 1"));
    assert!(rendered.contains("skipped: 0"));
    assert!(rendered.contains("accuracy: 0.0"));
    assert!(rendered.contains("coverage: 1.0"));
    assert!(rendered.contains(
        "FAIL comp_1 cap_rate expected=5.0% actual=5.5% compare_as=percent tolerance=0.01"
    ));

    Ok(())
}

#[test]
fn bench_u_render_report_human_renders_pass_without_detail_lines()
-> Result<(), Box<dyn std::error::Error>> {
    let rendered = render_report(&sample_report(AssertionOutcome::Pass), false)?;

    assert!(rendered.contains("BENCHMARK PASS"));
    assert!(rendered.contains("passed: 1"));
    assert!(rendered.contains("failed: 0"));
    assert!(rendered.contains("skipped: 0"));
    assert!(rendered.contains("accuracy: 1.0"));
    assert!(rendered.contains("coverage: 1.0"));
    assert!(!rendered.contains("\nFAIL "));
    assert!(!rendered.contains("\nSKIP "));
    assert_eq!(
        sample_report(AssertionOutcome::Pass).outcome,
        ReportOutcome::Pass
    );

    Ok(())
}
