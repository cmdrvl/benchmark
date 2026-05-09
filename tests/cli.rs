use std::{
    path::{Path, PathBuf},
    process::Command,
};

use benchmark::{
    Outcome,
    assertions::Severity,
    cli::Cli,
    compare::CompareAs,
    execute,
    lock_check::InputVerification,
    render::{RenderMode, render_report},
    report::{AssertionOutcome, BenchmarkReport, EvaluatedAssertion, ReportContext, ReportOutcome},
};
use clap::CommandFactory;
use clap::Parser;

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn fixture_cli(candidate: &str, assertions: &str, lockfiles: &[&str], json: bool) -> Cli {
    Cli {
        candidate: Some(fixture(candidate)),
        assertions: Some(fixture(assertions)),
        key: Some("comp_id".to_owned()),
        lock: lockfiles.iter().map(|path| fixture(path)).collect(),
        json,
        render: None,
        command: None,
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

#[allow(non_snake_case)]
#[test]
fn BENCH_I001_nested_candidate_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let execution = execute(fixture_cli(
        "tests/fixtures/candidates/refusal/bench_i004_nested.json",
        "tests/fixtures/assertions/smoke/bench_i001_gold.jsonl",
        &[],
        true,
    ))?;

    assert_eq!(execution.outcome, Outcome::Refusal);
    let json: serde_json::Value = serde_json::from_str(&execution.stdout)?;
    assert_eq!(json["refusal"]["code"], "E_FORMAT_DETECT");
    Ok(())
}

#[allow(non_snake_case)]
#[test]
fn BENCH_I002_missing_key_column_refuses_with_e_key_not_found()
-> Result<(), Box<dyn std::error::Error>> {
    let execution = execute(fixture_cli(
        "tests/fixtures/candidates/refusal/bench_no_comp_id.csv",
        "tests/fixtures/assertions/smoke/bench_i001_gold.jsonl",
        &[],
        true,
    ))?;

    assert_eq!(execution.outcome, Outcome::Refusal);
    let json: serde_json::Value = serde_json::from_str(&execution.stdout)?;
    assert_eq!(json["refusal"]["code"], "E_KEY_NOT_FOUND");
    Ok(())
}

#[allow(non_snake_case)]
#[test]
fn BENCH_I003_duplicate_key_rows_refuse_with_e_key_not_unique()
-> Result<(), Box<dyn std::error::Error>> {
    let execution = execute(fixture_cli(
        "tests/fixtures/candidates/refusal/bench_duplicate_key.csv",
        "tests/fixtures/assertions/smoke/bench_i001_gold.jsonl",
        &[],
        true,
    ))?;

    assert_eq!(execution.outcome, Outcome::Refusal);
    let json: serde_json::Value = serde_json::from_str(&execution.stdout)?;
    assert_eq!(json["refusal"]["code"], "E_KEY_NOT_UNIQUE");
    Ok(())
}

#[allow(non_snake_case)]
#[test]
fn BENCH_I004_null_or_blank_key_rows_refuse_with_e_key_null()
-> Result<(), Box<dyn std::error::Error>> {
    let execution = execute(fixture_cli(
        "tests/fixtures/candidates/refusal/bench_null_key.csv",
        "tests/fixtures/assertions/smoke/bench_i001_gold.jsonl",
        &[],
        true,
    ))?;

    assert_eq!(execution.outcome, Outcome::Refusal);
    let json: serde_json::Value = serde_json::from_str(&execution.stdout)?;
    assert_eq!(json["refusal"]["code"], "E_KEY_NULL");
    Ok(())
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
    assert!(help.contains("CANDIDATE"));
    assert!(help.contains("--assertions"));
    assert!(help.contains("--key"));
    assert!(help.contains("--lock"));
    assert!(help.contains("--json"));
    assert!(help.contains("--render"));
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
    assert_eq!(json["tool"], "benchmark");
    assert_eq!(json["version"], "benchmark.v0");
    assert_eq!(json["outcome"], "REFUSAL");
    assert_eq!(json["policy_signals"], serde_json::json!({}));
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
fn bench_i_pass_scoring_run_keeps_stderr_clean() -> Result<(), Box<dyn std::error::Error>> {
    let candidate = fixture("tests/fixtures/candidates/smoke/bench_i001_candidate.csv");
    let assertions = fixture("tests/fixtures/assertions/smoke/bench_i001_gold.jsonl");

    let output = Command::new(env!("CARGO_BIN_EXE_benchmark"))
        .arg(&candidate)
        .arg("--assertions")
        .arg(&assertions)
        .arg("--key")
        .arg("comp_id")
        .arg("--json")
        .output()?;

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty(), "stderr was not empty");

    Ok(())
}

#[test]
fn bench_i_fail_scoring_run_keeps_stderr_clean() -> Result<(), Box<dyn std::error::Error>> {
    let candidate = fixture("tests/fixtures/candidates/smoke/bench_mixed.csv");
    let assertions = fixture("tests/fixtures/assertions/smoke/bench_mixed_gold.jsonl");

    let output = Command::new(env!("CARGO_BIN_EXE_benchmark"))
        .arg(&candidate)
        .arg("--assertions")
        .arg(&assertions)
        .arg("--key")
        .arg("comp_id")
        .arg("--render")
        .arg("summary")
        .output()?;

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty(), "stderr was not empty");

    Ok(())
}

#[test]
fn bench_i_refusal_run_still_surfaces_refusal_stdout() -> Result<(), Box<dyn std::error::Error>> {
    let candidate = fixture("tests/fixtures/candidates/smoke/bench_i001_candidate.csv");
    let assertions = fixture("tests/fixtures/assertions/refusal/bench_u001_malformed.jsonl");

    let output = Command::new(env!("CARGO_BIN_EXE_benchmark"))
        .arg(&candidate)
        .arg("--assertions")
        .arg(&assertions)
        .arg("--key")
        .arg("comp_id")
        .output()?;

    assert_eq!(output.status.code(), Some(2));
    assert!(std::str::from_utf8(&output.stdout)?.contains("REFUSAL [E_BAD_ASSERTIONS]"));

    Ok(())
}

#[test]
fn bench_u_execute_returns_summary_fail_with_exit_1() -> Result<(), Box<dyn std::error::Error>> {
    let candidate = fixture("tests/fixtures/candidates/smoke/bench_mixed.csv");
    let expected_candidate = candidate.display().to_string();
    let execution = execute(Cli {
        candidate: Some(candidate),
        assertions: Some(fixture(
            "tests/fixtures/assertions/smoke/bench_mixed_gold.jsonl",
        )),
        key: Some("comp_id".to_owned()),
        lock: Vec::new(),
        json: false,
        render: Some(benchmark::cli::SummaryRenderMode::Summary),
        command: None,
    })?;

    assert_eq!(execution.outcome, Outcome::Fail);
    assert_eq!(execution.exit_code(), 1);
    assert_eq!(
        execution.stdout,
        format!(
            "tool=benchmark version=benchmark.v0 candidate={} outcome=FAIL accuracy=0.8 coverage=0.714 failed=1 skipped=2 quality_band=LOW refusal_code=-\n",
            expected_candidate
        )
    );

    Ok(())
}

#[test]
fn bench_u_json_and_render_flags_conflict() {
    let err = Cli::try_parse_from([
        "benchmark",
        "candidate.csv",
        "--assertions",
        "gold.jsonl",
        "--key",
        "comp_id",
        "--json",
        "--render",
        "summary",
    ])
    .expect_err("json and render should conflict");

    assert!(err.to_string().contains("--json"));
    assert!(err.to_string().contains("--render"));
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
    assert_eq!(json["tool"], "benchmark");
    assert_eq!(json["outcome"], "PASS");
    assert_eq!(json["policy_signals"]["quality_band"], "HIGH");
    assert_eq!(
        json["policy_signals"]["quality_band_basis"],
        "all_pass_no_skip"
    );
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
    assert!(
        execution
            .stdout
            .contains("quality_band: LOW (assertion_failures_present)")
    );
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

#[allow(non_snake_case)]
#[test]
fn BENCH_I011_repeated_json_runs_are_byte_identical() -> Result<(), Box<dyn std::error::Error>> {
    let cli = fixture_cli(
        "tests/fixtures/candidates/smoke/bench_mixed.csv",
        "tests/fixtures/assertions/smoke/bench_mixed_gold.jsonl",
        &[],
        true,
    );

    let first = execute(cli.clone())?;
    let second = execute(cli)?;

    assert_eq!(first.outcome, Outcome::Fail);
    assert_eq!(first.stdout, second.stdout);
    Ok(())
}

#[allow(non_snake_case)]
#[test]
fn BENCH_I012_human_and_json_modes_reflect_same_failures_and_skips()
-> Result<(), Box<dyn std::error::Error>> {
    let human = execute(fixture_cli(
        "tests/fixtures/candidates/smoke/bench_mixed.csv",
        "tests/fixtures/assertions/smoke/bench_mixed_gold.jsonl",
        &[],
        false,
    ))?;
    let json = execute(fixture_cli(
        "tests/fixtures/candidates/smoke/bench_mixed.csv",
        "tests/fixtures/assertions/smoke/bench_mixed_gold.jsonl",
        &[],
        true,
    ))?;
    let report: serde_json::Value = serde_json::from_str(&json.stdout)?;

    assert_eq!(human.outcome, Outcome::Fail);
    assert_eq!(json.outcome, Outcome::Fail);
    assert!(human.stdout.contains("failed: 1"));
    assert!(human.stdout.contains("skipped: 2"));
    assert!(
        human
            .stdout
            .contains("quality_band: LOW (assertion_failures_present)")
    );
    assert_eq!(report["tool"], "benchmark");
    assert_eq!(report["policy_signals"]["quality_band"], "LOW");
    assert_eq!(
        report["policy_signals"]["quality_band_basis"],
        "assertion_failures_present"
    );
    assert_eq!(report["summary"]["failed"], 1);
    assert_eq!(report["summary"]["skipped"], 2);
    assert!(human.stdout.contains("FAIL comp_3 cap_rate"));
    assert!(
        human
            .stdout
            .contains("SKIP comp_7 cap_rate reason=SKIP_ENTITY")
    );
    assert!(
        human
            .stdout
            .contains("SKIP comp_4 nonexistent_field reason=SKIP_FIELD")
    );
    assert_eq!(report["failures"][0]["entity"], "comp_3");
    assert_eq!(report["skipped"][0]["entity"], "comp_7");
    assert_eq!(report["skipped"][1]["entity"], "comp_4");
    Ok(())
}

#[test]
fn bench_u_render_report_json_preserves_machine_contract() -> Result<(), Box<dyn std::error::Error>>
{
    let rendered = render_report(&sample_report(AssertionOutcome::Fail), RenderMode::Json)?;
    let json: serde_json::Value = serde_json::from_str(&rendered)?;

    assert_eq!(json["version"], "benchmark.v0");
    assert_eq!(json["tool"], "benchmark");
    assert_eq!(json["outcome"], "FAIL");
    assert_eq!(json["candidate"], "normalized.csv");
    assert_eq!(json["candidate_hash"], "sha256:candidate");
    assert_eq!(json["assertions_hash"], "sha256:assertions");
    assert_eq!(json["policy_signals"]["quality_band"], "LOW");
    assert_eq!(
        json["policy_signals"]["quality_band_basis"],
        "assertion_failures_present"
    );
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
    let rendered = render_report(&sample_report(AssertionOutcome::Fail), RenderMode::Human)?;

    assert!(rendered.contains("BENCHMARK FAIL"));
    assert!(rendered.contains("candidate: normalized.csv"));
    assert!(rendered.contains("assertions: gold.jsonl"));
    assert!(rendered.contains("key: comp_id"));
    assert!(rendered.contains("passed: 0"));
    assert!(rendered.contains("failed: 1"));
    assert!(rendered.contains("skipped: 0"));
    assert!(rendered.contains("quality_band: LOW (assertion_failures_present)"));
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
    let rendered = render_report(&sample_report(AssertionOutcome::Pass), RenderMode::Human)?;

    assert!(rendered.contains("BENCHMARK PASS"));
    assert!(rendered.contains("passed: 1"));
    assert!(rendered.contains("failed: 0"));
    assert!(rendered.contains("skipped: 0"));
    assert!(rendered.contains("quality_band: HIGH (all_pass_no_skip)"));
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

#[test]
fn bench_u_render_report_summary_tsv_has_stable_columns() -> Result<(), Box<dyn std::error::Error>>
{
    let rendered = render_report(
        &sample_report(AssertionOutcome::Fail),
        RenderMode::SummaryTsv,
    )?;

    assert_eq!(
        rendered,
        "tool\tversion\tcandidate\toutcome\taccuracy\tcoverage\tfailed\tskipped\tquality_band\trefusal_code\nbenchmark\tbenchmark.v0\tnormalized.csv\tFAIL\t0.0\t1.0\t1\t0\tLOW\t-\n"
    );
    Ok(())
}
