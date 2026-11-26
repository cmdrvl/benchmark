#![forbid(unsafe_code)]

use std::{collections::HashSet, fs, path::PathBuf};

pub const TOOL: &str = "benchmark";
pub const REPORT_VERSION: &str = "benchmark.v0";
pub const E_PROFILE_INVALID: &str = "E_PROFILE_INVALID";
pub const E_REDACTION_KEY_INVALID: &str = "E_REDACTION_KEY_INVALID";

/// benchmark's operator.v1 manifest. `--describe` and `capabilities --json`
/// both print exactly this document.
pub const OPERATOR_JSON: &str = include_str!("../operator.json");

/// The operator manifest as printed on stdout (single trailing newline).
pub fn describe_output() -> String {
    let mut out = OPERATOR_JSON.trim_end().to_owned();
    out.push('\n');
    out
}

pub mod assertions;
pub mod candidate;
pub mod cli;
pub mod compare;
pub mod doctor;
pub mod engine;
pub mod key_check;
pub mod lock_check;
pub mod paths;
pub mod refusal;
pub mod render;
pub mod report;
pub mod witness;

/// JSON Schema for `benchmark.v0` stdout (score report or refusal envelope).
pub const SCHEMA_JSON: &str = include_str!("../benchmark.schema.json");

use assertions::AssertionError;
use candidate::{CandidateError, LoadedCandidate};
use clap::CommandFactory;
use cli::{BenchmarkCommand, Cli, Command, DoctorArgs, DoctorCommand, RobotDocsCommand};
use cmdrvl_core::redact::RedactionKey;
use engine::EngineError;
use key_check::KeyCheckError;
use lock_check::{LockCheckError, verify_candidate};
use refusal::RefusalEnvelope;
use report::RedactionMode;
use serde::Deserialize;
use serde_json::{Value, json};
use thiserror::Error;

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
    if cli.describe {
        if has_scoring_args(&cli) || cli.command.is_some() || cli.robot_triage {
            return Err("`benchmark --describe` does not accept other arguments".into());
        }
        return Ok(Execution::new(Outcome::Pass, describe_output()));
    }

    if cli.schema {
        if has_scoring_args(&cli) || cli.command.is_some() || cli.robot_triage {
            return Err("`benchmark --schema` does not accept other arguments".into());
        }
        let mut schema = SCHEMA_JSON.trim_end().to_owned();
        schema.push('\n');
        return Ok(Execution::new(Outcome::Pass, schema));
    }

    if cli.robot_triage {
        if has_scoring_args(&cli) {
            return Err(
                "`benchmark --robot-triage` does not accept candidate, assertions, key, profile, explicit, lock, or render arguments"
                    .into(),
            );
        }

        return Ok(doctor::execute(
            DoctorArgs {
                robot_triage: true,
                command: None,
            },
            true,
        ));
    }

    if let Some(command) = cli.command.take() {
        if has_scoring_args(&cli) {
            return Err(
                "`benchmark doctor`, `benchmark capabilities`, and `benchmark robot-docs` do not accept candidate, assertions, key, lock, or render arguments"
                    .into(),
            );
        }

        return match command {
            Command::Doctor(doctor) => Ok(doctor::execute(doctor, cli.json)),
            Command::Capabilities => {
                if cli.json {
                    Ok(Execution::new(Outcome::Pass, describe_output()))
                } else {
                    Ok(doctor::execute(
                        DoctorArgs {
                            robot_triage: false,
                            command: Some(DoctorCommand::Capabilities),
                        },
                        false,
                    ))
                }
            }
            Command::Witness { action } => witness::execute(&action, cli.json).map_err(|error| {
                format!("benchmark: witness: failed to read ledger: {error}").into()
            }),
            Command::RobotDocs(args) => {
                let command = args.command.unwrap_or(RobotDocsCommand::Guide);
                match command {
                    RobotDocsCommand::Guide => Ok(doctor::execute(
                        DoctorArgs {
                            robot_triage: false,
                            command: Some(DoctorCommand::RobotDocs),
                        },
                        false,
                    )),
                }
            }
        };
    }

    if cli.candidate.is_none()
        && cli.assertions.is_none()
        && cli.key.is_none()
        && cli.sheet.is_none()
        && cli.profile.is_none()
        && cli.lock.is_empty()
        && !cli.explicit
    {
        if cli.json {
            return Ok(Execution::new(Outcome::Pass, describe_output()));
        }

        return Ok(Execution::new(Outcome::Pass, long_help()));
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

fn has_scoring_args(cli: &Cli) -> bool {
    cli.candidate.is_some()
        || cli.assertions.is_some()
        || cli.key.is_some()
        || cli.sheet.is_some()
        || cli.profile.is_some()
        || cli.explicit
        || !cli.lock.is_empty()
        || cli.render.is_some()
}

fn long_help() -> String {
    let mut command = Cli::command();
    let mut help = command.render_long_help().to_string();
    if !help.ends_with('\n') {
        help.push('\n');
    }
    help
}

fn execute_command(
    command: &BenchmarkCommand,
) -> Result<report::BenchmarkReport, Box<RefusalEnvelope>> {
    let redaction_mode = resolve_redaction_mode(command)?;
    let key_columns = resolve_key_columns(command)
        .map_err(|error| Box::new(refusal_from_profile_error(command, &error)))?;
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

    let candidate = LoadedCandidate::load_with_sheet(&command.candidate, command.sheet.as_deref())
        .map_err(|error| Box::new(refusal_from_candidate_error(command, &error)))?;
    let key_check = key_check::validate_keys(&candidate, key_columns)
        .map_err(|error| Box::new(refusal_from_key_check_error(command, &error)))?;

    engine::score_candidate_with_redaction(
        &candidate,
        &command.assertions,
        &assertions,
        &key_check,
        input_verification,
        redaction_mode,
    )
    .map_err(|error| Box::new(refusal_from_engine_error(command, &error)))
}

fn resolve_redaction_mode(
    command: &BenchmarkCommand,
) -> Result<RedactionMode, Box<RefusalEnvelope>> {
    let configured_key = match std::env::var("BENCHMARK_REDACTION_KEY") {
        Ok(key) => Some(key),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(Box::new(refusal_from_parts(
                command,
                E_REDACTION_KEY_INVALID,
                "BENCHMARK_REDACTION_KEY must be valid UTF-8 and at least 16 bytes".to_owned(),
                json!({}),
            )));
        }
    };

    // Core says only why a key is unusable (empty or too short); the code and message are ours.
    let key = RedactionKey::from_optional(configured_key.as_deref().map(str::as_bytes)).map_err(
        |_| {
            Box::new(refusal_from_parts(
                command,
                E_REDACTION_KEY_INVALID,
                "BENCHMARK_REDACTION_KEY must be at least 16 bytes".to_owned(),
                json!({}),
            ))
        },
    )?;

    if command.explicit {
        Ok(RedactionMode::explicit())
    } else {
        Ok(match key {
            Some(key) => RedactionMode::keyed(key),
            None => RedactionMode::ordinal(),
        })
    }
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
        CandidateError::Tabular { path, detail } => (
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

    let ambiguous_sheet = ["E", "AMBIGUOUS_SHEET"].join("_");
    let next = if matches!(error, CandidateError::Tabular { detail, .. } if detail.contains(&ambiguous_sheet))
    {
        format!("{} --sheet <name|index>", next_command(command))
    } else {
        next_command(command)
    };
    RefusalEnvelope::new(code, error.to_string(), detail, Some(next))
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
        LockCheckError::UnsupportedHashAlgorithm {
            candidate,
            lockfile,
            member,
            algorithm,
        } => json!({
            "candidate": candidate.display().to_string(),
            "lockfile": lockfile.display().to_string(),
            "member": member,
            "algorithm": algorithm,
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
        EngineError::InvalidCompositeEntity {
            entity,
            expected_components,
            actual_components,
        } => (
            assertions::E_BAD_ASSERTIONS,
            json!({
                "entity": entity,
                "expected_components": expected_components,
                "actual_components": actual_components,
            }),
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
    ];

    if let Some(profile) = &command.profile {
        parts.push("--profile".to_owned());
        parts.push(shell_quote(&profile.display().to_string()));
    } else {
        parts.push("--key".to_owned());
        parts.push(shell_quote(&command.key));
    }

    if let Some(sheet) = &command.sheet {
        parts.push("--sheet".to_owned());
        parts.push(shell_quote(sheet));
    }

    for lockfile in &command.lockfiles {
        parts.push("--lock".to_owned());
        parts.push(shell_quote(&lockfile.display().to_string()));
    }

    parts.push("--json".to_owned());
    if command.explicit {
        parts.push("--explicit".to_owned());
    }

    parts.join(" ")
}

#[derive(Debug, Error)]
enum ProfileError {
    #[error("failed to read profile '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse profile '{path}': {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid profile '{path}': {message}")]
    Invalid { path: PathBuf, message: String },
}

#[derive(Debug, Deserialize)]
struct ProfileFile {
    key: Option<Vec<String>>,
    key_columns: Option<Vec<String>>,
}

fn resolve_key_columns(command: &BenchmarkCommand) -> Result<Vec<String>, ProfileError> {
    if command.profile.is_some() && !command.key.is_empty() {
        return Err(ProfileError::Invalid {
            path: command.profile.clone().unwrap_or_default(),
            message: "--key and --profile cannot be combined".to_owned(),
        });
    }

    if let Some(path) = &command.profile {
        let contents = fs::read_to_string(path).map_err(|source| ProfileError::Io {
            path: path.clone(),
            source,
        })?;
        let profile: ProfileFile =
            serde_json::from_str(&contents).map_err(|source| ProfileError::Parse {
                path: path.clone(),
                source,
            })?;
        let columns = match (profile.key, profile.key_columns) {
            (Some(_), Some(_)) => {
                return Err(ProfileError::Invalid {
                    path: path.clone(),
                    message: "profile must define only one of 'key' or 'key_columns'".to_owned(),
                });
            }
            (Some(columns), None) | (None, Some(columns)) => columns,
            (None, None) => {
                return Err(ProfileError::Invalid {
                    path: path.clone(),
                    message: "profile must define a non-empty 'key' or 'key_columns' array"
                        .to_owned(),
                });
            }
        };
        return normalize_key_columns(columns).map_err(|message| ProfileError::Invalid {
            path: path.clone(),
            message,
        });
    }

    if command.key.is_empty() {
        return Err(ProfileError::Invalid {
            path: PathBuf::from("--key"),
            message: "missing required argument --key <COLUMN> or --profile <FILE>".to_owned(),
        });
    }

    normalize_key_columns(command.key.split(',').map(str::to_owned).collect()).map_err(|message| {
        ProfileError::Invalid {
            path: PathBuf::from("--key"),
            message,
        }
    })
}

fn normalize_key_columns(columns: Vec<String>) -> Result<Vec<String>, String> {
    let mut normalized = Vec::with_capacity(columns.len());
    let mut seen = HashSet::with_capacity(columns.len());
    for column in columns {
        let column = column.trim_matches([' ', '\t']).to_owned();
        if column.is_empty() {
            return Err("key columns must not be empty".to_owned());
        }
        if !seen.insert(column.clone()) {
            return Err(format!("key column '{column}' was provided more than once"));
        }
        normalized.push(column);
    }
    if normalized.is_empty() {
        return Err("at least one key column is required".to_owned());
    }
    Ok(normalized)
}

fn refusal_from_profile_error(command: &BenchmarkCommand, error: &ProfileError) -> RefusalEnvelope {
    let detail = match error {
        ProfileError::Io { path, .. } | ProfileError::Parse { path, .. } => {
            json!({ "path": path.display().to_string() })
        }
        ProfileError::Invalid { path, message } => json!({
            "path": path.display().to_string(),
            "message": message,
        }),
    };
    refusal_from_parts(command, E_PROFILE_INVALID, error.to_string(), detail)
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
            sheet: None,
            profile: None,
            explicit: false,
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
            sheet: None,
            profile: None,
            explicit: false,
            lock: Vec::new(),
            json: true,
            render: None,
            robot_triage: false,
            describe: false,
            schema: false,
            no_witness: true,
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
            sheet: None,
            profile: None,
            explicit: false,
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
