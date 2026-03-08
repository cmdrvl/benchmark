use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub version: String,
    pub outcome: String,
    pub candidate: String,
    pub assertions_file: String,
    pub key_column: String,
    pub summary: Summary,
}

impl BenchmarkReport {
    pub fn scaffold(
        candidate: impl Into<String>,
        assertions_file: impl Into<String>,
        key_column: impl Into<String>,
    ) -> Self {
        Self {
            version: "benchmark.v0".to_owned(),
            outcome: "REFUSAL".to_owned(),
            candidate: candidate.into(),
            assertions_file: assertions_file.into(),
            key_column: key_column.into(),
            summary: Summary::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub total: u64,
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub resolved: u64,
    pub accuracy: Option<f64>,
    pub coverage: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureRecord {
    pub entity: String,
    pub field: String,
    pub expected: String,
    pub actual: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkipRecord {
    pub entity: String,
    pub field: String,
    pub reason: String,
}
