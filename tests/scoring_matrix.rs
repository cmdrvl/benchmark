use std::path::{Path, PathBuf};

use benchmark::{
    assertions::{AssertionSet, load_assertions},
    candidate::LoadedCandidate,
    engine::{evaluate_assertions, score_candidate},
    key_check::validate_key,
    report::{AssertionOutcome, BenchmarkReport, ReportOutcome, SkipReason, Summary},
};
use serde_json::{Value, json};

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn load_smoke_candidate() -> Result<LoadedCandidate, Box<dyn std::error::Error>> {
    Ok(LoadedCandidate::load(fixture(
        "tests/fixtures/candidates/smoke/bench_mixed.csv",
    ))?)
}

fn expected_report(name: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let path = fixture(&format!("tests/fixtures/expected/{name}"));
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn compact_failures(report: &BenchmarkReport) -> Value {
    Value::Array(
        report
            .failures
            .iter()
            .map(|failure| {
                json!({
                    "entity": failure.entity,
                    "field": failure.field,
                    "expected": failure.expected,
                    "actual": failure.actual,
                    "compare_as": failure.compare_as,
                    "tolerance": failure.tolerance,
                    "severity": failure.severity,
                    "source": failure.source,
                })
            })
            .collect(),
    )
}

fn compact_skips(report: &BenchmarkReport) -> Value {
    Value::Array(
        report
            .skipped
            .iter()
            .map(|skip| {
                json!({
                    "entity": skip.entity,
                    "field": skip.field,
                    "reason": skip.reason,
                })
            })
            .collect(),
    )
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[allow(non_snake_case)]
#[test]
fn BENCH_I005_missing_entity_produces_skip_entity_without_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let candidate = load_smoke_candidate()?;
    let assertions = load_assertions(fixture(
        "tests/fixtures/assertions/smoke/bench_mixed_gold.jsonl",
    ))?;
    let key_check = validate_key(&candidate, "comp_id")?;
    let single = AssertionSet {
        assertions: vec![assertions.assertions[4].clone()],
    };

    let evaluated = evaluate_assertions(&candidate, &single, &key_check)?;

    assert_eq!(evaluated.len(), 1);
    assert_eq!(evaluated[0].outcome, AssertionOutcome::SkipEntity);
    assert_eq!(
        evaluated[0].detail.as_deref(),
        Some("Entity 'comp_7' not found in candidate")
    );

    let summary = Summary::from_evaluated(&evaluated);
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.skipped, 1);
    assert_eq!(summary.resolved, 0);
    assert_eq!(summary.accuracy, None);
    Ok(())
}

#[allow(non_snake_case)]
#[test]
fn BENCH_I006_missing_field_produces_skip_field_without_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let candidate = load_smoke_candidate()?;
    let assertions = load_assertions(fixture(
        "tests/fixtures/assertions/smoke/bench_mixed_gold.jsonl",
    ))?;
    let key_check = validate_key(&candidate, "comp_id")?;
    let single = AssertionSet {
        assertions: vec![assertions.assertions[5].clone()],
    };

    let evaluated = evaluate_assertions(&candidate, &single, &key_check)?;

    assert_eq!(evaluated.len(), 1);
    assert_eq!(evaluated[0].outcome, AssertionOutcome::SkipField);
    assert_eq!(
        evaluated[0].detail.as_deref(),
        Some("Field 'nonexistent_field' not found in candidate")
    );

    let summary = Summary::from_evaluated(&evaluated);
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.skipped, 1);
    assert_eq!(summary.resolved, 0);
    assert_eq!(summary.accuracy, None);
    Ok(())
}

#[allow(non_snake_case)]
#[test]
fn BENCH_I007_all_skipped_report_keeps_accuracy_null() -> Result<(), Box<dyn std::error::Error>> {
    let candidate = load_smoke_candidate()?;
    let assertions_path = fixture("tests/fixtures/assertions/smoke/bench_all_skip_gold.jsonl");
    let assertions = load_assertions(&assertions_path)?;
    let key_check = validate_key(&candidate, "comp_id")?;

    let report = score_candidate(&candidate, &assertions_path, &assertions, &key_check, None)?;
    let expected = expected_report("bench_all_skip.json")?;

    assert_eq!(report.outcome, ReportOutcome::Fail);
    assert_eq!(report.summary.total, 2);
    assert_eq!(report.summary.passed, 0);
    assert_eq!(report.summary.failed, 0);
    assert_eq!(report.summary.skipped, 2);
    assert_eq!(report.summary.resolved, 0);
    assert_eq!(report.summary.accuracy, None);
    assert_eq!(report.summary.coverage, 0.0);
    assert_eq!(report.summary.by_severity.critical.skipped, 1);
    assert_eq!(report.summary.by_severity.major.skipped, 1);
    assert_eq!(report.summary.by_severity.minor.skipped, 0);
    assert!(report.failures.is_empty());
    assert_eq!(report.skipped.len(), 2);
    assert_eq!(report.skipped[0].reason, SkipReason::SkipEntity);
    assert_eq!(report.skipped[1].reason, SkipReason::SkipEntity);
    assert!(report.candidate_hash.starts_with("sha256:"));
    assert!(report.assertions_hash.starts_with("sha256:"));

    assert_eq!(report.version, expected["version"]);
    assert_eq!(report.outcome, ReportOutcome::Fail);
    assert_eq!(report.key_column, expected["key_column"]);
    assert_eq!(
        report.summary.total,
        expected["summary"]["total"].as_u64().unwrap()
    );
    assert_eq!(
        report.summary.passed,
        expected["summary"]["passed"].as_u64().unwrap()
    );
    assert_eq!(
        report.summary.failed,
        expected["summary"]["failed"].as_u64().unwrap()
    );
    assert_eq!(
        report.summary.skipped,
        expected["summary"]["skipped"].as_u64().unwrap()
    );
    assert_eq!(
        report.summary.resolved,
        expected["summary"]["resolved"].as_u64().unwrap()
    );
    assert_eq!(compact_failures(&report), expected["failures"]);
    assert_eq!(compact_skips(&report), expected["skipped"]);
    Ok(())
}

#[allow(non_snake_case)]
#[test]
fn BENCH_I008_mixed_results_compute_summary_and_preserve_order()
-> Result<(), Box<dyn std::error::Error>> {
    let candidate = load_smoke_candidate()?;
    let assertions_path = fixture("tests/fixtures/assertions/smoke/bench_mixed_gold.jsonl");
    let assertions = load_assertions(&assertions_path)?;
    let key_check = validate_key(&candidate, "comp_id")?;

    let report = score_candidate(&candidate, &assertions_path, &assertions, &key_check, None)?;
    let expected = expected_report("bench_mixed_fail.json")?;

    assert_eq!(report.outcome, ReportOutcome::Fail);
    assert_eq!(report.summary.total, 7);
    assert_eq!(report.summary.passed, 4);
    assert_eq!(report.summary.failed, 1);
    assert_eq!(report.summary.skipped, 2);
    assert_eq!(report.summary.resolved, 5);
    assert_eq!(report.summary.accuracy, Some(0.8));
    assert!((report.summary.coverage - (5.0 / 7.0)).abs() < 1e-12);
    assert_eq!(report.summary.by_severity.critical.passed, 1);
    assert_eq!(report.summary.by_severity.critical.failed, 0);
    assert_eq!(report.summary.by_severity.critical.skipped, 0);
    assert_eq!(report.summary.by_severity.major.passed, 3);
    assert_eq!(report.summary.by_severity.major.failed, 1);
    assert_eq!(report.summary.by_severity.major.skipped, 0);
    assert_eq!(report.summary.by_severity.minor.passed, 0);
    assert_eq!(report.summary.by_severity.minor.failed, 0);
    assert_eq!(report.summary.by_severity.minor.skipped, 2);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].entity, "comp_3");
    assert_eq!(report.failures[0].field, "cap_rate");
    assert_eq!(report.failures[0].actual.as_deref(), Some("7.25%"));
    assert_eq!(report.skipped.len(), 2);
    assert_eq!(report.skipped[0].entity, "comp_7");
    assert_eq!(report.skipped[0].reason, SkipReason::SkipEntity);
    assert_eq!(report.skipped[1].entity, "comp_4");
    assert_eq!(report.skipped[1].reason, SkipReason::SkipField);
    assert!(report.candidate_hash.starts_with("sha256:"));
    assert!(report.assertions_hash.starts_with("sha256:"));

    assert_eq!(report.version, expected["version"]);
    assert_eq!(report.outcome, ReportOutcome::Fail);
    assert_eq!(report.key_column, expected["key_column"]);
    assert_eq!(
        report.summary.total,
        expected["summary"]["total"].as_u64().unwrap()
    );
    assert_eq!(
        report.summary.passed,
        expected["summary"]["passed"].as_u64().unwrap()
    );
    assert_eq!(
        report.summary.failed,
        expected["summary"]["failed"].as_u64().unwrap()
    );
    assert_eq!(
        report.summary.skipped,
        expected["summary"]["skipped"].as_u64().unwrap()
    );
    assert_eq!(
        report.summary.resolved,
        expected["summary"]["resolved"].as_u64().unwrap()
    );
    assert_eq!(
        report.summary.accuracy,
        expected["summary"]["accuracy"].as_f64()
    );
    assert_eq!(
        round3(report.summary.coverage),
        expected["summary"]["coverage"].as_f64().unwrap()
    );
    assert_eq!(compact_failures(&report), expected["failures"]);
    assert_eq!(compact_skips(&report), expected["skipped"]);
    Ok(())
}
