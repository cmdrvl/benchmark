use crate::{
    assertions::AssertionSet, candidate::CandidateSource, key_check::KeyCheckResult,
    lock_check::InputVerification, report::BenchmarkReport,
};

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkRequest {
    pub candidate: CandidateSource,
    pub assertions: AssertionSet,
    pub key_check: KeyCheckResult,
    pub input_verification: Option<InputVerification>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineState {
    pub request: BenchmarkRequest,
    pub report: BenchmarkReport,
}
