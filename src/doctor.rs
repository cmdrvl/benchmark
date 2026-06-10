use serde_json::{Value, json};

use crate::{
    Execution, Outcome, REPORT_VERSION, TOOL,
    cli::{DoctorArgs, DoctorCommand},
    paths,
};

const CONTRACT: &str = "cmdrvl.read_only_doctor.v1";
const HEALTH_SCHEMA: &str = "benchmark.doctor.health.v1";
const CAPABILITIES_SCHEMA: &str = "benchmark.doctor.capabilities.v1";
const TRIAGE_SCHEMA: &str = "benchmark.doctor.triage.v1";

pub fn execute(args: DoctorArgs, json: bool) -> Execution {
    let command = if args.robot_triage {
        DoctorCommand::RobotDocs
    } else {
        args.command.unwrap_or(DoctorCommand::Health)
    };

    match (command, args.robot_triage, json) {
        (_, true, _) => json_execution(triage_payload()),
        (DoctorCommand::Health, _, true) => json_execution(health_payload()),
        (DoctorCommand::Health, _, false) => {
            Execution::new(Outcome::Pass, health_summary(&health_payload()))
        }
        (DoctorCommand::Capabilities, _, true) => json_execution(capabilities_payload()),
        (DoctorCommand::Capabilities, _, false) => Execution::new(
            Outcome::Pass,
            "benchmark doctor capabilities\nread_only=true\nfixers=0\ncommands=health,capabilities,robot-docs,--robot-triage",
        ),
        (DoctorCommand::RobotDocs, _, _) => Execution::new(Outcome::Pass, robot_docs_text()),
    }
}

fn json_execution(payload: Value) -> Execution {
    Execution::new(Outcome::Pass, json_string(&payload))
}

fn json_string(payload: &Value) -> String {
    match serde_json::to_string(payload) {
        Ok(encoded) => encoded,
        Err(error) => format!(
            "{{\"schema\":\"{HEALTH_SCHEMA}\",\"contract\":\"{CONTRACT}\",\"tool\":\"benchmark\",\"ok\":false,\"error\":\"json encode failed: {error}\"}}"
        ),
    }
}

fn health_payload() -> Value {
    let checks = health_checks();
    let failed = checks
        .iter()
        .filter(|check| !check.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let total = checks.len();
    let passed = total.saturating_sub(failed);

    json!({
        "schema": HEALTH_SCHEMA,
        "contract": CONTRACT,
        "tool": TOOL,
        "version": env!("CARGO_PKG_VERSION"),
        "report_version": REPORT_VERSION,
        "ok": failed == 0,
        "read_only": true,
        "summary": {
            "checks_total": total,
            "checks_passed": passed,
            "checks_failed": failed
        },
        "checks": checks,
        "observed_paths": observed_paths(),
        "config_footprint": paths::config_footprint(),
        "side_effects": side_effects(),
        "fixers": []
    })
}

fn capabilities_payload() -> Value {
    json!({
        "schema": CAPABILITIES_SCHEMA,
        "contract": CONTRACT,
        "tool": TOOL,
        "version": env!("CARGO_PKG_VERSION"),
        "report_version": REPORT_VERSION,
        "read_only": true,
        "agent_entrypoints": [
            {
                "usage": "benchmark --robot-triage",
                "output_schema": TRIAGE_SCHEMA,
                "description": "Single-call read-only health, side-effect, and next-step triage for automation"
            },
            {
                "usage": "benchmark capabilities --json",
                "output_schema": CAPABILITIES_SCHEMA,
                "description": "Top-level alias for the full machine-readable CLI contract"
            },
            {
                "usage": "benchmark robot-docs guide",
                "output_schema": "text/plain",
                "description": "Top-level alias for the agent-oriented command guide"
            }
        ],
        "commands": [
            {
                "name": "run-json",
                "usage": "benchmark <CANDIDATE> --assertions <FILE> --key <COLUMN> --json",
                "output_schema": "benchmark.v0",
                "description": "Score one row-oriented candidate against a JSONL assertion set"
            },
            {
                "name": "run-summary",
                "usage": "benchmark <CANDIDATE> --assertions <FILE> --key <COLUMN> --render summary",
                "output_schema": "text/plain",
                "description": "Emit a one-line operator summary derived from the score report"
            },
            {
                "name": "run-summary-tsv",
                "usage": "benchmark <CANDIDATE> --assertions <FILE> --key <COLUMN> --render summary-tsv",
                "output_schema": "text/tab-separated-values",
                "description": "Emit a stable TSV header and row for shell pipelines"
            },
            {
                "name": "run-with-lock",
                "usage": "benchmark <CANDIDATE> --assertions <FILE> --key <COLUMN> --lock <LOCKFILE> --json",
                "output_schema": "benchmark.v0",
                "description": "Verify candidate lock membership and hash before scoring"
            },
            {
                "name": "top-level-triage",
                "usage": "benchmark --robot-triage",
                "output_schema": TRIAGE_SCHEMA,
                "description": "Top-level compact triage report for automation"
            },
            {
                "name": "top-level-capabilities",
                "usage": "benchmark capabilities --json",
                "output_schema": CAPABILITIES_SCHEMA,
                "description": "Top-level capabilities alias"
            },
            {
                "name": "top-level-robot-docs",
                "usage": "benchmark robot-docs guide",
                "output_schema": "text/plain",
                "description": "Top-level agent guide alias"
            },
            {
                "name": "health",
                "usage": "benchmark doctor health --json",
                "output_schema": HEALTH_SCHEMA,
                "description": "Report compiled crate, report contract, and read-only doctor health"
            },
            {
                "name": "capabilities",
                "usage": "benchmark doctor capabilities --json",
                "output_schema": CAPABILITIES_SCHEMA,
                "description": "Describe doctor commands, exit codes, side-effect boundaries, and disabled fixers"
            },
            {
                "name": "robot-docs",
                "usage": "benchmark doctor robot-docs",
                "output_schema": "text/plain",
                "description": "Emit concise machine-oriented usage notes"
            },
            {
                "name": "robot-triage",
                "usage": "benchmark doctor --robot-triage",
                "output_schema": TRIAGE_SCHEMA,
                "description": "Emit a compact triage report for automation"
            }
        ],
        "exit_codes": {
            "0": "PASS scoring outcome or successful read-only metadata command",
            "1": "FAIL scoring outcome: one or more assertions failed or skipped",
            "2": "REFUSAL, CLI usage error, or unexpected top-level error"
        },
        "refusal_codes": [
            {
                "code": "E_IO",
                "meaning": "candidate, assertions, or lockfile could not be read",
                "next_command": "benchmark <CANDIDATE> --assertions <FILE> --key <COLUMN> --json"
            },
            {
                "code": "E_BAD_ASSERTIONS",
                "meaning": "assertions JSONL is malformed or semantically invalid",
                "next_command": "benchmark <CANDIDATE> --assertions <FIXED_FILE> --key <COLUMN> --json"
            },
            {
                "code": "E_EMPTY_ASSERTIONS",
                "meaning": "assertions file contains zero valid assertions",
                "next_command": "benchmark <CANDIDATE> --assertions <NONEMPTY_FILE> --key <COLUMN> --json"
            },
            {
                "code": "E_KEY_NOT_FOUND",
                "meaning": "candidate does not contain the requested key column",
                "next_command": "benchmark <CANDIDATE> --assertions <FILE> --key <EXISTING_COLUMN> --json"
            },
            {
                "code": "E_KEY_NOT_UNIQUE",
                "meaning": "candidate key values are ambiguous",
                "next_command": "benchmark <CANONICALIZED_CANDIDATE> --assertions <FILE> --key <UNIQUE_COLUMN> --json"
            },
            {
                "code": "E_KEY_NULL",
                "meaning": "candidate key values include null or blank values",
                "next_command": "benchmark <CLEAN_CANDIDATE> --assertions <FILE> --key <COLUMN> --json"
            },
            {
                "code": "E_FORMAT_DETECT",
                "meaning": "candidate format is unsupported or not one row-oriented relation",
                "next_command": "benchmark <ROW_ORIENTED_CSV_JSON_JSONL_OR_PARQUET> --assertions <FILE> --key <COLUMN> --json"
            },
            {
                "code": "E_INPUT_NOT_LOCKED",
                "meaning": "candidate is not present in supplied lockfiles",
                "next_command": "benchmark <CANDIDATE> --assertions <FILE> --key <COLUMN> --lock <CORRECT_LOCKFILE> --json"
            },
            {
                "code": "E_INPUT_DRIFT",
                "meaning": "candidate hash does not match the lock member",
                "next_command": "benchmark <LOCKED_CANDIDATE> --assertions <FILE> --key <COLUMN> --lock <LOCKFILE> --json"
            }
        ],
        "config_footprint": paths::config_footprint(),
        "side_effects": side_effects(),
        "fixers": []
    })
}

fn triage_payload() -> Value {
    let health = health_payload();
    let ok = health.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let checks = health
        .get("checks")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));

    json!({
        "schema": TRIAGE_SCHEMA,
        "contract": CONTRACT,
        "tool": TOOL,
        "version": env!("CARGO_PKG_VERSION"),
        "report_version": REPORT_VERSION,
        "ok": ok,
        "score": if ok { 100 } else { 0 },
        "read_only": true,
        "checks": checks,
        "config_footprint": paths::config_footprint(),
        "side_effects": side_effects(),
        "fixers": [],
        "recommended_next_steps": [
            "Use benchmark capabilities --json for the machine-readable CLI contract.",
            "Use benchmark robot-docs guide for the agent-oriented command guide.",
            "Use benchmark <candidate> --assertions <gold.jsonl> --key <column> --json for a score report.",
            "Do not expect benchmark doctor to read candidate files, assertion files, lockfiles, open DuckDB, score assertions, or mint gold truth."
        ]
    })
}

fn health_checks() -> Vec<Value> {
    vec![
        check(
            "crate_version_embedded",
            !env!("CARGO_PKG_VERSION").is_empty(),
            "compiled Cargo package version is present",
        ),
        check(
            "tool_identity",
            TOOL == "benchmark",
            "compiled tool identity is benchmark",
        ),
        check(
            "report_contract",
            REPORT_VERSION == "benchmark.v0",
            "compiled scoring report contract is benchmark.v0",
        ),
        check(
            "fix_mode_disabled",
            true,
            "doctor --fix is intentionally absent from the CLI surface",
        ),
        check(
            "candidate_loader_unentered",
            true,
            "doctor does not detect formats, open DuckDB, or load candidate relations",
        ),
        check(
            "assertion_loader_unentered",
            true,
            "doctor does not open or parse assertion JSONL files",
        ),
        check(
            "lock_verification_unentered",
            true,
            "doctor does not read lockfiles or hash candidate inputs",
        ),
        check(
            "scoring_engine_unentered",
            true,
            "doctor does not validate keys, compare values, or construct score reports",
        ),
        check(
            "gold_truth_not_minted",
            true,
            "doctor does not promote prior outputs into benchmark assertions",
        ),
        check(
            "network_disabled",
            true,
            "doctor performs no DNS, HTTP, TLS, or other network probes",
        ),
        check(
            "config_footprint_declared",
            true,
            "benchmark has no implicit managed config, state, or cache paths",
        ),
    ]
}

fn check(id: &str, ok: bool, message: &str) -> Value {
    json!({
        "id": id,
        "ok": ok,
        "severity": if ok { "info" } else { "error" },
        "message": message
    })
}

fn observed_paths() -> Value {
    json!({
        "crate_metadata": "compiled:CARGO_PKG_VERSION",
        "report_contract": "compiled:benchmark.v0",
        "candidate": null,
        "assertions": null,
        "lockfiles": [],
        "managed_config_paths": [],
        "managed_state_paths": [],
        "managed_cache_paths": []
    })
}

fn side_effects() -> Value {
    json!({
        "reads_config_files": false,
        "writes_config_files": false,
        "writes_migration_logs": false,
        "writes_deprecation_notices": false,
        "reads_stdin": false,
        "reads_candidate_files": false,
        "reads_assertion_files": false,
        "reads_lockfiles": false,
        "detects_candidate_format": false,
        "opens_duckdb_connection": false,
        "loads_candidate_relation": false,
        "validates_key": false,
        "verifies_lock_membership": false,
        "scores_assertions": false,
        "renders_score_report": false,
        "promotes_gold_truth": false,
        "writes_candidate_files": false,
        "writes_assertion_files": false,
        "writes_lockfiles": false,
        "writes_doctor_artifacts": false,
        "uses_network": false,
        "changes_cwd": false
    })
}

fn health_summary(payload: &Value) -> String {
    let ok = payload.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let passed = payload
        .get("summary")
        .and_then(|summary| summary.get("checks_passed"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total = payload
        .get("summary")
        .and_then(|summary| summary.get("checks_total"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let status = if ok { "ok" } else { "unhealthy" };
    format!("benchmark doctor health: {status}\nread_only=true\nfixers=0\nchecks={passed}/{total}")
}

fn robot_docs_text() -> &'static str {
    "benchmark robot-docs guide\n\
contract: cmdrvl.read_only_doctor.v1\n\
agent_entrypoints:\n\
  benchmark --robot-triage\n\
  benchmark capabilities --json\n\
  benchmark robot-docs guide\n\
scoring:\n\
  benchmark <CANDIDATE> --assertions <FILE> --key <COLUMN> --json\n\
  benchmark <CANDIDATE> --assertions <FILE> --key <COLUMN> --render summary\n\
  benchmark <CANDIDATE> --assertions <FILE> --key <COLUMN> --lock <LOCKFILE> --json\n\
commands:\n\
  benchmark doctor health --json\n\
  benchmark doctor capabilities --json\n\
  benchmark doctor --robot-triage\n\
read_only:\n\
  - does not read stdin, candidates, assertions, lockfiles, or fixture paths\n\
  - does not detect formats, open DuckDB, validate keys, verify locks, compare values, score assertions, or mint gold truth\n\
  - does not create directories, write doctor artifacts, or use the network\n\
  - no implicit ~/.cmdrvl config, state, or cache paths are read or written\n\
fix_mode:\n\
  - no --fix surface is implemented in this release\n\
next_steps:\n\
  - use benchmark capabilities --json for the machine-readable CLI contract\n\
  - use benchmark <candidate> --assertions <gold.jsonl> --key <column> --json for score reports"
}
