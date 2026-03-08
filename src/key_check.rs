#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCheckResult {
    pub key_column: String,
    pub benchmarked_rows: usize,
}
