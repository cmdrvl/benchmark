use serde::{Deserialize, Serialize};

use crate::{
    REPORT_VERSION, TOOL, assertions::Severity, compare::CompareAs, lock_check::InputVerification,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReportOutcome {
    Pass,
    Fail,
    Refusal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssertionOutcome {
    Pass,
    Fail,
    SkipEntity,
    SkipField,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SkipReason {
    SkipEntity,
    SkipField,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportContext {
    pub candidate: String,
    pub candidate_hash: String,
    pub assertions_file: String,
    pub assertions_hash: String,
    pub key_column: String,
    pub input_verification: Option<InputVerification>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub tool: String,
    pub version: String,
    pub outcome: ReportOutcome,
    pub candidate: String,
    pub candidate_hash: String,
    pub assertions_file: String,
    pub assertions_hash: String,
    pub key_column: String,
    pub input_verification: Option<InputVerification>,
    pub policy_signals: PolicySignals,
    pub summary: Summary,
    pub failures: Vec<FailureRecord>,
    pub skipped: Vec<SkipRecord>,
    pub refusal: Option<()>,
}

impl BenchmarkReport {
    pub fn from_evaluated(context: ReportContext, evaluated: Vec<EvaluatedAssertion>) -> Self {
        let summary = Summary::from_evaluated(&evaluated);
        let outcome = if summary.failed == 0 && summary.skipped == 0 {
            ReportOutcome::Pass
        } else {
            ReportOutcome::Fail
        };

        let failures = evaluated
            .iter()
            .filter_map(EvaluatedAssertion::failure_record)
            .collect();
        let skipped = evaluated
            .iter()
            .filter_map(EvaluatedAssertion::skip_record)
            .collect();

        Self {
            tool: TOOL.to_owned(),
            version: REPORT_VERSION.to_owned(),
            outcome,
            candidate: context.candidate,
            candidate_hash: context.candidate_hash,
            assertions_file: context.assertions_file,
            assertions_hash: context.assertions_hash,
            key_column: context.key_column,
            input_verification: context.input_verification,
            policy_signals: PolicySignals::from_summary(&summary),
            summary,
            failures,
            skipped,
            refusal: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub total: u64,
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub resolved: u64,
    pub accuracy: Option<f64>,
    pub coverage: f64,
    pub by_severity: SeverityBreakdown,
}

impl Default for Summary {
    fn default() -> Self {
        Self {
            total: 0,
            passed: 0,
            failed: 0,
            skipped: 0,
            resolved: 0,
            accuracy: None,
            coverage: 0.0,
            by_severity: SeverityBreakdown::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QualityBand {
    High,
    Acceptable,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityBandBasis {
    AllPassNoSkip,
    SkipOnly,
    AssertionFailuresPresent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicySignals {
    pub quality_band: QualityBand,
    pub quality_band_basis: QualityBandBasis,
}

impl PolicySignals {
    pub fn from_summary(summary: &Summary) -> Self {
        if summary.failed > 0 {
            return Self {
                quality_band: QualityBand::Low,
                quality_band_basis: QualityBandBasis::AssertionFailuresPresent,
            };
        }

        if summary.skipped > 0 {
            return Self {
                quality_band: QualityBand::Acceptable,
                quality_band_basis: QualityBandBasis::SkipOnly,
            };
        }

        Self {
            quality_band: QualityBand::High,
            quality_band_basis: QualityBandBasis::AllPassNoSkip,
        }
    }
}

impl Summary {
    pub fn from_evaluated(evaluated: &[EvaluatedAssertion]) -> Self {
        let mut summary = Self {
            total: evaluated.len() as u64,
            ..Self::default()
        };

        for assertion in evaluated {
            match assertion.outcome {
                AssertionOutcome::Pass => {
                    summary.passed += 1;
                    summary
                        .by_severity
                        .record(assertion.severity, assertion.outcome);
                }
                AssertionOutcome::Fail => {
                    summary.failed += 1;
                    summary
                        .by_severity
                        .record(assertion.severity, assertion.outcome);
                }
                AssertionOutcome::SkipEntity | AssertionOutcome::SkipField => {
                    summary.skipped += 1;
                    summary
                        .by_severity
                        .record(assertion.severity, assertion.outcome);
                }
            }
        }

        summary.resolved = summary.passed + summary.failed;
        summary.accuracy = if summary.resolved == 0 {
            None
        } else {
            Some(summary.passed as f64 / summary.resolved as f64)
        };
        summary.coverage = if summary.total == 0 {
            0.0
        } else {
            summary.resolved as f64 / summary.total as f64
        };

        summary
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeverityBreakdown {
    pub critical: SeverityCounts,
    pub major: SeverityCounts,
    pub minor: SeverityCounts,
}

impl SeverityBreakdown {
    fn record(&mut self, severity: Severity, outcome: AssertionOutcome) {
        let bucket = match severity {
            Severity::Critical => &mut self.critical,
            Severity::Major => &mut self.major,
            Severity::Minor => &mut self.minor,
        };

        match outcome {
            AssertionOutcome::Pass => bucket.passed += 1,
            AssertionOutcome::Fail => bucket.failed += 1,
            AssertionOutcome::SkipEntity | AssertionOutcome::SkipField => bucket.skipped += 1,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeverityCounts {
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvaluatedAssertion {
    pub entity: String,
    pub field: String,
    pub expected: String,
    pub actual: Option<String>,
    pub compare_as: CompareAs,
    pub tolerance: Option<f64>,
    pub severity: Severity,
    pub source: Option<String>,
    pub outcome: AssertionOutcome,
    pub detail: Option<String>,
}

impl EvaluatedAssertion {
    fn failure_record(&self) -> Option<FailureRecord> {
        if self.outcome != AssertionOutcome::Fail {
            return None;
        }

        Some(FailureRecord {
            entity: self.entity.clone(),
            field: self.field.clone(),
            expected: self.expected.clone(),
            actual: self.actual.clone(),
            compare_as: self.compare_as,
            tolerance: self.tolerance,
            severity: self.severity,
            source: self.source.clone(),
        })
    }

    fn skip_record(&self) -> Option<SkipRecord> {
        let reason = match self.outcome {
            AssertionOutcome::SkipEntity => SkipReason::SkipEntity,
            AssertionOutcome::SkipField => SkipReason::SkipField,
            AssertionOutcome::Pass | AssertionOutcome::Fail => return None,
        };

        Some(SkipRecord {
            entity: self.entity.clone(),
            field: self.field.clone(),
            reason,
            detail: self.detail.clone().unwrap_or_default(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailureRecord {
    pub entity: String,
    pub field: String,
    pub expected: String,
    pub actual: Option<String>,
    pub compare_as: CompareAs,
    pub tolerance: Option<f64>,
    pub severity: Severity,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkipRecord {
    pub entity: String,
    pub field: String,
    pub reason: SkipReason,
    pub detail: String,
}

#[cfg(test)]
mod tests {
    use crate::{
        assertions::Severity,
        compare::CompareAs,
        report::{
            AssertionOutcome, BenchmarkReport, EvaluatedAssertion, PolicySignals, QualityBand,
            QualityBandBasis, ReportContext, ReportOutcome, SkipReason, Summary,
        },
    };

    fn evaluated(
        entity: &str,
        field: &str,
        severity: Severity,
        outcome: AssertionOutcome,
    ) -> EvaluatedAssertion {
        EvaluatedAssertion {
            entity: entity.to_owned(),
            field: field.to_owned(),
            expected: "expected".to_owned(),
            actual: Some("actual".to_owned()),
            compare_as: CompareAs::String,
            tolerance: None,
            severity,
            source: None,
            outcome,
            detail: Some(format!("{field} detail")),
        }
    }

    #[test]
    fn bench_u_report_summary_math_keeps_accuracy_and_coverage_separate() {
        let report = BenchmarkReport::from_evaluated(
            ReportContext {
                candidate: "candidate.csv".to_owned(),
                candidate_hash: "sha256:candidate".to_owned(),
                assertions_file: "gold.jsonl".to_owned(),
                assertions_hash: "sha256:assertions".to_owned(),
                key_column: "comp_id".to_owned(),
                input_verification: None,
            },
            vec![
                evaluated(
                    "comp_1",
                    "property_name",
                    Severity::Critical,
                    AssertionOutcome::Pass,
                ),
                evaluated(
                    "comp_2",
                    "cap_rate",
                    Severity::Major,
                    AssertionOutcome::Fail,
                ),
                evaluated(
                    "comp_3",
                    "as_of",
                    Severity::Minor,
                    AssertionOutcome::SkipEntity,
                ),
                evaluated(
                    "comp_4",
                    "missing_field",
                    Severity::Major,
                    AssertionOutcome::SkipField,
                ),
            ],
        );

        assert_eq!(report.outcome, ReportOutcome::Fail);
        assert_eq!(report.tool, "benchmark");
        assert_eq!(report.summary.total, 4);
        assert_eq!(report.summary.passed, 1);
        assert_eq!(report.summary.failed, 1);
        assert_eq!(report.summary.skipped, 2);
        assert_eq!(report.summary.resolved, 2);
        assert_eq!(report.summary.accuracy, Some(0.5));
        assert_eq!(report.summary.coverage, 0.5);
        assert_eq!(report.summary.by_severity.critical.passed, 1);
        assert_eq!(report.summary.by_severity.major.failed, 1);
        assert_eq!(report.summary.by_severity.major.skipped, 1);
        assert_eq!(report.summary.by_severity.minor.skipped, 1);
        assert_eq!(
            report.policy_signals,
            PolicySignals {
                quality_band: QualityBand::Low,
                quality_band_basis: QualityBandBasis::AssertionFailuresPresent,
            }
        );
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.skipped.len(), 2);
        assert_eq!(report.skipped[0].reason, SkipReason::SkipEntity);
        assert_eq!(report.skipped[1].reason, SkipReason::SkipField);
    }

    #[test]
    fn bench_u_report_accuracy_is_null_when_everything_skips() {
        let report = BenchmarkReport::from_evaluated(
            ReportContext {
                candidate: "candidate.csv".to_owned(),
                candidate_hash: "sha256:candidate".to_owned(),
                assertions_file: "gold.jsonl".to_owned(),
                assertions_hash: "sha256:assertions".to_owned(),
                key_column: "comp_id".to_owned(),
                input_verification: None,
            },
            vec![evaluated(
                "comp_7",
                "cap_rate",
                Severity::Major,
                AssertionOutcome::SkipEntity,
            )],
        );

        assert_eq!(report.outcome, ReportOutcome::Fail);
        assert_eq!(report.summary.total, 1);
        assert_eq!(report.summary.resolved, 0);
        assert_eq!(report.summary.accuracy, None);
        assert_eq!(report.summary.coverage, 0.0);
        assert_eq!(
            report.policy_signals,
            PolicySignals {
                quality_band: QualityBand::Acceptable,
                quality_band_basis: QualityBandBasis::SkipOnly,
            }
        );
    }

    #[test]
    fn bench_u_policy_signals_are_deterministic_functions_of_summary() {
        let high = PolicySignals::from_summary(&Summary {
            total: 2,
            passed: 2,
            failed: 0,
            skipped: 0,
            resolved: 2,
            accuracy: Some(1.0),
            coverage: 1.0,
            by_severity: Default::default(),
        });
        let acceptable = PolicySignals::from_summary(&Summary {
            total: 2,
            passed: 1,
            failed: 0,
            skipped: 1,
            resolved: 1,
            accuracy: Some(1.0),
            coverage: 0.5,
            by_severity: Default::default(),
        });
        let low = PolicySignals::from_summary(&Summary {
            total: 2,
            passed: 1,
            failed: 1,
            skipped: 0,
            resolved: 2,
            accuracy: Some(0.5),
            coverage: 1.0,
            by_severity: Default::default(),
        });

        assert_eq!(
            high,
            PolicySignals {
                quality_band: QualityBand::High,
                quality_band_basis: QualityBandBasis::AllPassNoSkip,
            }
        );
        assert_eq!(
            acceptable,
            PolicySignals {
                quality_band: QualityBand::Acceptable,
                quality_band_basis: QualityBandBasis::SkipOnly,
            }
        );
        assert_eq!(
            low,
            PolicySignals {
                quality_band: QualityBand::Low,
                quality_band_basis: QualityBandBasis::AssertionFailuresPresent,
            }
        );
    }
}
