use serde_json::{Value, json};

use crate::{
    Execution, Outcome, REPORT_VERSION, TOOL,
    cli::{DoctorArgs, DoctorCommand},
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
        "commands": [
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
            "0": "doctor report emitted successfully",
            "1": "reserved for benchmark scoring FAIL outcomes, not used by read-only doctor",
            "2": "CLI usage error, benchmark refusal, or unexpected top-level error"
        },
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
        "side_effects": side_effects(),
        "fixers": [],
        "recommended_next_steps": [
            "Use benchmark --help for the scoring CLI contract.",
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
        "lockfiles": []
    })
}

fn side_effects() -> Value {
    json!({
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
    "benchmark doctor robot-docs\n\
contract: cmdrvl.read_only_doctor.v1\n\
commands:\n\
  benchmark doctor health --json\n\
  benchmark doctor capabilities --json\n\
  benchmark doctor --robot-triage\n\
read_only:\n\
  - does not read stdin, candidates, assertions, lockfiles, or fixture paths\n\
  - does not detect formats, open DuckDB, validate keys, verify locks, compare values, score assertions, or mint gold truth\n\
  - does not create directories, write doctor artifacts, or use the network\n\
fix_mode:\n\
  - no --fix surface is implemented in this release\n\
next_steps:\n\
  - use benchmark --help for the scoring CLI contract\n\
  - use benchmark <candidate> --assertions <gold.jsonl> --key <column> --json for score reports"
}
