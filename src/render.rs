use crate::{
    refusal::RefusalEnvelope,
    report::{BenchmarkReport, ReportOutcome, SkipReason},
};

pub fn render_report(
    report: &BenchmarkReport,
    json_mode: bool,
) -> Result<String, serde_json::Error> {
    if json_mode {
        let mut rendered = serde_json::to_string_pretty(report)?;
        rendered.push('\n');
        return Ok(rendered);
    }

    Ok(render_report_human(report))
}

pub fn render_refusal(
    refusal: &RefusalEnvelope,
    json_mode: bool,
) -> Result<String, serde_json::Error> {
    refusal.render(json_mode)
}

fn render_report_human(report: &BenchmarkReport) -> String {
    let mut lines = vec![
        format!("BENCHMARK {}", outcome_label(report.outcome)),
        format!("candidate: {}", report.candidate),
        format!("assertions: {}", report.assertions_file),
        format!("key: {}", report.key_column),
        format!("passed: {}", report.summary.passed),
        format!("failed: {}", report.summary.failed),
        format!("skipped: {}", report.summary.skipped),
        format!(
            "accuracy: {}",
            format_optional_score(report.summary.accuracy)
        ),
        format!("coverage: {}", format_score(report.summary.coverage)),
    ];

    let mut detail_lines = report
        .failures
        .iter()
        .map(render_failure)
        .collect::<Vec<_>>();
    detail_lines.extend(report.skipped.iter().map(render_skip));

    if !detail_lines.is_empty() {
        lines.push(String::new());
        lines.extend(detail_lines);
    }

    let mut rendered = lines.join("\n");
    rendered.push('\n');
    rendered
}

fn render_failure(failure: &crate::report::FailureRecord) -> String {
    let actual = failure.actual.as_deref().unwrap_or("null");
    let mut rendered = format!(
        "FAIL {} {} expected={} actual={} compare_as={}",
        failure.entity,
        failure.field,
        failure.expected,
        actual,
        failure.compare_as.label(),
    );

    if let Some(tolerance) = failure.tolerance {
        rendered.push_str(&format!(" tolerance={}", format_tolerance(tolerance)));
    }

    rendered
}

fn render_skip(skip: &crate::report::SkipRecord) -> String {
    format!(
        "SKIP {} {} reason={}",
        skip.entity,
        skip.field,
        skip_reason_label(skip.reason),
    )
}

fn outcome_label(outcome: ReportOutcome) -> &'static str {
    match outcome {
        ReportOutcome::Pass => "PASS",
        ReportOutcome::Fail => "FAIL",
        ReportOutcome::Refusal => "REFUSAL",
    }
}

fn skip_reason_label(reason: SkipReason) -> &'static str {
    match reason {
        SkipReason::SkipEntity => "SKIP_ENTITY",
        SkipReason::SkipField => "SKIP_FIELD",
    }
}

fn format_optional_score(value: Option<f64>) -> String {
    match value {
        Some(value) => format_score(value),
        None => "null".to_owned(),
    }
}

fn format_score(value: f64) -> String {
    trim_decimal(format!("{value:.3}"), true)
}

fn format_tolerance(value: f64) -> String {
    trim_decimal(value.to_string(), false)
}

fn trim_decimal(mut rendered: String, keep_trailing_zero: bool) -> String {
    if rendered.contains('.') {
        while rendered.ends_with('0') {
            rendered.pop();
        }

        if rendered.ends_with('.') {
            if keep_trailing_zero {
                rendered.push('0');
            } else {
                rendered.pop();
            }
        }
    }

    rendered
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{
        assertions::Severity,
        compare::CompareAs,
        lock_check::InputVerification,
        render::render_report,
        report::{AssertionOutcome, BenchmarkReport, EvaluatedAssertion, ReportContext},
    };

    fn sample_report() -> BenchmarkReport {
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
            vec![
                EvaluatedAssertion {
                    entity: "comp_3".to_owned(),
                    field: "cap_rate".to_owned(),
                    expected: "5.0%".to_owned(),
                    actual: Some("5.5%".to_owned()),
                    compare_as: CompareAs::Percent,
                    tolerance: Some(0.01),
                    severity: Severity::Major,
                    source: Some("reference_excel:E18".to_owned()),
                    outcome: AssertionOutcome::Fail,
                    detail: None,
                },
                EvaluatedAssertion {
                    entity: "comp_7".to_owned(),
                    field: "cap_rate".to_owned(),
                    expected: "5.0%".to_owned(),
                    actual: None,
                    compare_as: CompareAs::Percent,
                    tolerance: Some(0.01),
                    severity: Severity::Major,
                    source: None,
                    outcome: AssertionOutcome::SkipEntity,
                    detail: Some("Entity 'comp_7' not found in candidate".to_owned()),
                },
            ],
        )
    }

    #[test]
    fn bench_u_render_report_json_round_trips_full_contract()
    -> Result<(), Box<dyn std::error::Error>> {
        let rendered = render_report(&sample_report(), true)?;
        let json: serde_json::Value = serde_json::from_str(&rendered)?;

        assert_eq!(json["version"], "benchmark.v0");
        assert_eq!(json["outcome"], "FAIL");
        assert_eq!(json["candidate"], "normalized.csv");
        assert_eq!(json["key_column"], "comp_id");
        assert_eq!(json["summary"]["failed"], 1);
        assert_eq!(json["summary"]["skipped"], 1);
        assert_eq!(json["summary"]["by_severity"]["major"]["failed"], 1);
        assert_eq!(json["summary"]["by_severity"]["major"]["skipped"], 1);
        assert_eq!(
            json["input_verification"]["matched_lockfile"],
            "candidate.lock.json"
        );
        assert_eq!(json["failures"][0]["compare_as"], "percent");
        assert_eq!(json["skipped"][0]["reason"], "SKIP_ENTITY");
        assert_eq!(json["refusal"], serde_json::Value::Null);
        Ok(())
    }

    #[test]
    fn bench_u_render_report_human_emits_compact_summary_and_details()
    -> Result<(), Box<dyn std::error::Error>> {
        let rendered = render_report(&sample_report(), false)?;

        assert!(rendered.contains("BENCHMARK FAIL"));
        assert!(rendered.contains("candidate: normalized.csv"));
        assert!(rendered.contains("assertions: gold.jsonl"));
        assert!(rendered.contains("key: comp_id"));
        assert!(rendered.contains("passed: 0"));
        assert!(rendered.contains("failed: 1"));
        assert!(rendered.contains("skipped: 1"));
        assert!(rendered.contains("accuracy: 0.0"));
        assert!(rendered.contains("coverage: 0.5"));
        assert!(rendered.contains(
            "FAIL comp_3 cap_rate expected=5.0% actual=5.5% compare_as=percent tolerance=0.01"
        ));
        assert!(rendered.contains("SKIP comp_7 cap_rate reason=SKIP_ENTITY"));
        Ok(())
    }
}
