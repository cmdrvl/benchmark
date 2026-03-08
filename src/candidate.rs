use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateFormat {
    Csv,
    Json,
    Jsonl,
    Parquet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSource {
    pub path: PathBuf,
    pub format: CandidateFormat,
}
