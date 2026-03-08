use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputVerification {
    pub lockfiles: Vec<PathBuf>,
    pub verified_member: Option<String>,
}
