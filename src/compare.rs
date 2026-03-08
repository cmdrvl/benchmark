use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareAs {
    #[default]
    String,
    Number,
    Percent,
    Date,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComparisonOutcome {
    pub matched: bool,
}
