use crate::compare::CompareAs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assertion {
    pub entity: String,
    pub field: String,
    pub expected: String,
    #[serde(default)]
    pub compare_as: CompareAs,
    pub tolerance: Option<String>,
    #[serde(default)]
    pub severity: Severity,
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssertionSet {
    pub assertions: Vec<Assertion>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    #[default]
    Major,
    Minor,
}
