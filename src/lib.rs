#![forbid(unsafe_code)]

pub const TOOL: &str = "benchmark";
pub const REPORT_VERSION: &str = "benchmark.v0";

pub mod assertions;
pub mod candidate;
pub mod cli;
pub mod compare;
pub mod doctor;
pub mod engine;
pub mod key_check;
pub mod lock_check;
pub mod refusal;
pub mod render;
pub mod report;

use assertions::AssertionError;
use candidate::{CandidateError, LoadedCandidate};
use cli::{BenchmarkCommand, Cli, Command};
use engine::EngineError;
use key_check::KeyCheckError;
use lock_check::{LockCheckError, verify_candidate};
use refusal::RefusalEnvelope;
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Pass,
    Fail,
    Refusal,
}

impl Outcome {
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Fail => 1,
            Self::Refusal => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    pub outcome: Outcome,
    pub stdout: String,
}

impl Execution {
    pub fn new(outcome: Outcome, stdout: impl Into<String>) -> Self {
        Self {
            outcome,
            stdout: stdout.into(),
        }
    }

    pub const fn exit_code(&self) -> u8 {
        self.outcome.exit_code()
    }
}

pub fn execute(mut cli: Cli) -> Result<Execution, Box<dyn std::error::Error>> {
    if let Some(Command::Doctor(doctor)) = cli.command.take() {
        if cli.candidate.is_some()
            || cli.assertions.is_some()
            || cli.key.is_some()
            || !cli.lock.is_empty()
            || cli.render.is_some()
        {
            return Err(
                "`benchmark doctor` does not accept candidate, assertions, key, lock, or render arguments"
                    .into(),
            );
        }

        return Ok(doctor::execute(doctor, cli.json));
    }

    let command = BenchmarkCommand::try_from_cli(cli)?;
    match execute_command(&command) {
        Ok(report) => Ok(Execution {
            outcome: Outcome::from(report.outcome),
            stdout: render::render_report(&report, command.render_mode)?,
        }),
        Err(refusal) => Ok(Execution {
            outcome: Outcome::Refusal,
            stdout: render::render_refusal(&refusal, command.render_mode)?,
        }),
    }
}

fn execute_command(
    command: &BenchmarkCommand,
) -> Result<report::BenchmarkReport, Box<RefusalEnvelope>> {
    let assertions = assertions::load_assertions(&command.assertions)
        .map_err(|error| Box::new(refusal_from_assertion_error(command, &error)))?;

    let input_verification = if command.lockfiles.is_empty() {
        None
    } else {
        Some(
            verify_candidate(&command.candidate, &command.lockfiles)
                .map_err(|error| Box::new(refusal_from_lock_error(command, &error)))?,
        )
    };

    let candidate = LoadedCandidate::load(&command.candidate)
        .map_err(|error| Box::new(refusal_from_candidate_error(command, &error)))?;
    let key_check = key_check::validate_key(&candidate, &command.key)
        .map_err(|error| Box::new(refusal_from_key_check_error(command, &error)))?;

    engine::score_candidate(
        &candidate,
        &command.assertions,
        &assertions,
        &key_check,
        input_verification,
    )
    .map_err(|error| Box::new(refusal_from_engine_error(command, &error)))
}

impl From<report::ReportOutcome> for Outcome {
    fn from(value: report::ReportOutcome) -> Self {
        match value {
            report::ReportOutcome::Pass => Self::Pass,
            report::ReportOutcome::Fail => Self::Fail,
            report::ReportOutcome::Refusal => Self::Refusal,
        }
    }
}

fn refusal_from_assertion_error(
    command: &BenchmarkCommand,
    error: &AssertionError,
) -> RefusalEnvelope {
    let detail = match error {
        AssertionError::Io { path, .. } => json!({ "path": path.display().to_string() }),
        AssertionError::Parse { path, line, .. } => {
            json!({ "path": path.display().to_string(), "line": line })
        }
        AssertionError::Semantic {
            path,
            line,
            message,
        } => json!({
            "path": path.display().to_string(),
            "line": line,
            "message": message,
        }),
        AssertionError::Empty { path } => json!({ "path": path.display().to_string() }),
    };

    refusal_from_parts(command, error.refusal_code(), error.to_string(), detail)
}

fn refusal_from_candidate_error(
    command: &BenchmarkCommand,
    error: &CandidateError,
) -> RefusalEnvelope {
    let (code, detail) = match error {
        CandidateError::Io { path, .. } => (
            assertions::E_IO,
            json!({ "path": path.display().to_string() }),
        ),
        CandidateError::FormatDetect { path } => (
            "E_FORMAT_DETECT",
            json!({ "path": path.display().to_string() }),
        ),
        CandidateError::CandidateShape { path, detail } => (
            "E_FORMAT_DETECT",
            json!({
                "path": path.display().to_string(),
                "detail": detail,
            }),
        ),
        CandidateError::Load { path, .. } => (
            assertions::E_IO,
            json!({ "path": path.display().to_string() }),
        ),
    };

    refusal_from_parts(command, code, error.to_string(), detail)
}

fn refusal_from_key_check_error(
    command: &BenchmarkCommand,
    error: &KeyCheckError,
) -> RefusalEnvelope {
    refusal_from_parts(
        command,
        error.refusal_code(),
        error.to_string(),
        error.refusal_detail(),
    )
}

fn refusal_from_lock_error(command: &BenchmarkCommand, error: &LockCheckError) -> RefusalEnvelope {
    let detail = match error {
        LockCheckError::Io { path, .. } | LockCheckError::Parse { path, .. } => {
            json!({ "path": path.display().to_string() })
        }
        LockCheckError::InputNotLocked {
            candidate,
            lockfiles,
        } => json!({
            "candidate": candidate.display().to_string(),
            "lockfiles": paths_json(lockfiles),
        }),
        LockCheckError::AmbiguousMember { candidate, matches } => json!({
            "candidate": candidate.display().to_string(),
            "matches": matches,
        }),
        LockCheckError::InputDrift {
            candidate,
            lockfile,
            member,
            expected_hash,
            actual_hash,
        } => json!({
            "candidate": candidate.display().to_string(),
            "lockfile": lockfile.display().to_string(),
            "member": member,
            "expected_hash": expected_hash,
            "actual_hash": actual_hash,
        }),
    };

    refusal_from_parts(command, error.refusal_code(), error.to_string(), detail)
}

fn refusal_from_engine_error(command: &BenchmarkCommand, error: &EngineError) -> RefusalEnvelope {
    let (code, detail) = match error {
        EngineError::InvalidExpectedValue {
            entity,
            field,
            expected,
            compare_as,
            ..
        } => (
            assertions::E_BAD_ASSERTIONS,
            json!({
                "entity": entity,
                "field": field,
                "expected": expected,
                "compare_as": compare_as.label(),
            }),
        ),
        EngineError::InvalidComparisonConfiguration {
            entity,
            field,
            compare_as,
            ..
        } => (
            assertions::E_BAD_ASSERTIONS,
            json!({
                "entity": entity,
                "field": field,
                "compare_as": compare_as.label(),
            }),
        ),
        EngineError::Io { path, .. } => (
            assertions::E_IO,
            json!({ "path": path.display().to_string() }),
        ),
        EngineError::ProjectionIndexOverflow { index } => {
            (assertions::E_IO, json!({ "projection_index": index }))
        }
        EngineError::ProjectionCardinality { expected, actual } => (
            assertions::E_IO,
            json!({ "expected": expected, "actual": actual }),
        ),
        EngineError::Query { .. } => (assertions::E_IO, Value::Null),
    };

    refusal_from_parts(command, code, error.to_string(), detail)
}

fn refusal_from_parts(
    command: &BenchmarkCommand,
    code: &str,
    message: String,
    detail: Value,
) -> RefusalEnvelope {
    RefusalEnvelope::new(code, message, detail, Some(next_command(command)))
}

fn next_command(command: &BenchmarkCommand) -> String {
    let mut parts = vec![
        "benchmark".to_owned(),
        shell_quote(&command.candidate.display().to_string()),
        "--assertions".to_owned(),
        shell_quote(&command.assertions.display().to_string()),
        "--key".to_owned(),
        shell_quote(&command.key),
    ];

    for lockfile in &command.lockfiles {
        parts.push("--lock".to_owned());
        parts.push(shell_quote(&lockfile.display().to_string()));
    }

    parts.push("--json".to_owned());

    parts.join(" ")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn paths_json(paths: &[std::path::PathBuf]) -> Value {
    Value::Array(
        paths
            .iter()
            .map(|path| Value::String(path.display().to_string()))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{cli::Cli, render::RenderMode};
    use serde_json::json;

    use super::{Outcome, execute};

    #[test]
    fn shell_quote_wraps_single_quotes() {
        assert_eq!(super::shell_quote("a'b.csv"), "'a'\"'\"'b.csv'");
    }

    #[test]
    fn next_command_always_emits_json_mode() {
        let command = crate::cli::BenchmarkCommand {
            candidate: PathBuf::from("candidate.csv"),
            assertions: PathBuf::from("gold.jsonl"),
            key: "comp_id".to_owned(),
            lockfiles: vec![PathBuf::from("candidate.lock.json")],
            render_mode: RenderMode::Human,
        };

        assert_eq!(
            super::next_command(&command),
            "benchmark 'candidate.csv' --assertions 'gold.jsonl' --key 'comp_id' --lock 'candidate.lock.json' --json"
        );
    }

    #[test]
    fn execute_maps_missing_candidate_to_refusal() -> Result<(), Box<dyn std::error::Error>> {
        let cli = Cli {
            candidate: Some("candidate.csv".into()),
            assertions: Some("gold.jsonl".into()),
            key: Some("comp_id".to_owned()),
            lock: Vec::new(),
            json: true,
            render: None,
            command: None,
        };

        let execution = execute(cli)?;
        assert_eq!(execution.outcome, Outcome::Refusal);
        assert_eq!(execution.exit_code(), 2);

        let json: serde_json::Value = serde_json::from_str(&execution.stdout)?;
        assert_eq!(json["outcome"], "REFUSAL");
        assert_eq!(json["refusal"]["code"], "E_IO");
        Ok(())
    }

    #[test]
    fn refusal_from_parts_keeps_json_contract() -> Result<(), Box<dyn std::error::Error>> {
        let command = crate::cli::BenchmarkCommand {
            candidate: PathBuf::from("candidate.csv"),
            assertions: PathBuf::from("gold.jsonl"),
            key: "comp_id".to_owned(),
            lockfiles: Vec::new(),
            render_mode: RenderMode::Json,
        };

        let refusal = super::refusal_from_parts(
            &command,
            "E_IO",
            "candidate file is unreadable".to_owned(),
            json!({ "path": "candidate.csv" }),
        );

        let rendered = refusal.render(RenderMode::Json)?;
        let envelope: serde_json::Value = serde_json::from_str(&rendered)?;
        assert_eq!(envelope["version"], "benchmark.v0");
        assert_eq!(envelope["outcome"], "REFUSAL");
        assert_eq!(envelope["refusal"]["code"], "E_IO");
        Ok(())
    }
}
