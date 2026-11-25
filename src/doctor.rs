//! `benchmark doctor`: read-only reports under the family `cmdrvl.doctor.v1` schema.
//!
//! | invocation                               | `mode`         |
//! |------------------------------------------|----------------|
//! | `doctor --json` / `doctor health --json` | `health`       |
//! | `doctor capabilities --json`             | `capabilities` (embeds the operator.v1 `contract`) |
//! | `doctor triage --json`                   | `triage`       |
//!
//! Aliases kept for compatibility: `--robot-triage` and `doctor --robot-triage`
//! emit the triage document. Exit `0` healthy, `1` unhealthy, `2` usage error.

use std::io::{self, IsTerminal};

use serde_json::{Value, json};

use crate::{
    Execution, Outcome, REPORT_VERSION, TOOL,
    cli::{DoctorArgs, DoctorCommand},
    paths,
};

pub const DOCTOR_SCHEMA_VERSION: &str = "cmdrvl.doctor.v1";
const FORCE_UNHEALTHY_ENV: &str = "BENCHMARK_DOCTOR_FORCE_UNHEALTHY";

pub fn execute(args: DoctorArgs, json: bool) -> Execution {
    let command = if args.robot_triage {
        DoctorCommand::Triage
    } else {
        args.command.unwrap_or(DoctorCommand::Health)
    };

    match (command, json || args.robot_triage) {
        (DoctorCommand::Health, true) => json_execution(health_payload()),
        (DoctorCommand::Health, false) => {
            let payload = health_payload();
            execution_for(&payload, health_summary(&payload))
        }
        (DoctorCommand::Capabilities, true) => json_execution(capabilities_payload()),
        (DoctorCommand::Capabilities, false) => Execution::new(
            Outcome::Pass,
            "benchmark doctor capabilities\nread_only=true\nfixers=0\ncommands=health,capabilities,triage,robot-docs,--robot-triage",
        ),
        (DoctorCommand::Triage, true) => json_execution(triage_payload()),
        (DoctorCommand::Triage, false) => {
            let payload = triage_payload();
            execution_for(&payload, triage_text(&payload))
        }
        (DoctorCommand::RobotDocs, _) => Execution::new(Outcome::Pass, robot_docs_text()),
    }
}

fn is_healthy(payload: &Value) -> bool {
    payload
        .get("healthy")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn outcome_for(payload: &Value) -> Outcome {
    if is_healthy(payload) {
        Outcome::Pass
    } else {
        Outcome::Fail
    }
}

fn execution_for(payload: &Value, text: String) -> Execution {
    Execution::new(outcome_for(payload), text)
}

fn json_execution(payload: Value) -> Execution {
    let outcome = outcome_for(&payload);
    Execution::new(outcome, json_string(&payload))
}

fn json_string(payload: &Value) -> String {
    match serde_json::to_string(payload) {
        Ok(encoded) => encoded,
        Err(error) => format!(
            "{{\"schema_version\":\"{DOCTOR_SCHEMA_VERSION}\",\"mode\":\"health\",\"tool\":\"benchmark\",\"tool_version\":\"{}\",\"healthy\":false,\"findings\":[{{\"id\":\"json_encode\",\"severity\":\"error\",\"message\":\"json encode failed: {error}\"}}]}}",
            env!("CARGO_PKG_VERSION")
        ),
    }
}

fn base(mode: &str, healthy: bool, findings: Vec<Value>) -> serde_json::Map<String, Value> {
    let mut object = serde_json::Map::new();
    object.insert("schema_version".into(), json!(DOCTOR_SCHEMA_VERSION));
    object.insert("mode".into(), json!(mode));
    object.insert("tool".into(), json!(TOOL));
    object.insert("tool_version".into(), json!(env!("CARGO_PKG_VERSION")));
    object.insert("healthy".into(), json!(healthy));
    object.insert("findings".into(), Value::Array(findings));
    object
}

fn findings_from_checks(checks: &[Value]) -> Vec<Value> {
    checks
        .iter()
        .map(|check| {
            json!({
                "id": check["id"],
                "severity": check["severity"],
                "message": check["message"],
            })
        })
        .collect()
}

fn health_payload() -> Value {
    let checks = health_checks();
    let failed = checks
        .iter()
        .filter(|check| !check.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let total = checks.len();
    let healthy = failed == 0;

    let mut object = base("health", healthy, findings_from_checks(&checks));
    // Additive detail kept from the pre-v1 health report.
    object.insert("report_version".into(), json!(REPORT_VERSION));
    object.insert("ok".into(), json!(healthy));
    object.insert("read_only".into(), json!(true));
    object.insert(
        "summary".into(),
        json!({
            "checks_total": total,
            "checks_passed": total.saturating_sub(failed),
            "checks_failed": failed
        }),
    );
    object.insert("checks".into(), Value::Array(checks));
    object.insert("observed_paths".into(), observed_paths());
    object.insert("config_footprint".into(), paths::config_footprint());
    object.insert("side_effects".into(), side_effects());
    object.insert("fixers".into(), json!([]));
    Value::Object(object)
}

/// `doctor capabilities`: no health inspection, so healthy with no findings; the
/// embedded `contract` is exactly `capabilities --json`.
fn capabilities_payload() -> Value {
    let contract: Value = serde_json::from_str(crate::OPERATOR_JSON).unwrap_or(Value::Null);
    let mut object = base("capabilities", true, Vec::new());
    object.insert("contract".into(), contract);
    Value::Object(object)
}

fn triage_payload() -> Value {
    let health = health_payload();
    let healthy = is_healthy(&health);
    let findings = health
        .get("findings")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new()));

    let mut next_actions = vec![
        "Use benchmark capabilities --json for the machine-readable CLI contract.".to_owned(),
        "Use benchmark robot-docs guide for the agent-oriented command guide.".to_owned(),
        "Use benchmark <candidate> --assertions <gold.jsonl> (--key <column>[,<column>...] | --profile <file>) --json for a score report.".to_owned(),
    ];
    if !healthy {
        next_actions.insert(
            0,
            "Reinstall benchmark: a compiled-in identity or contract check failed.".to_owned(),
        );
    }

    let mut object = base("triage", healthy, Vec::new());
    object.insert("findings".into(), findings);
    object.insert("next_actions".into(), json!(next_actions));
    // Additive detail kept from the pre-v1 triage report.
    object.insert("report_version".into(), json!(REPORT_VERSION));
    object.insert("ok".into(), json!(healthy));
    object.insert("score".into(), json!(if healthy { 100 } else { 0 }));
    object.insert("read_only".into(), json!(true));
    object.insert(
        "checks".into(),
        health.get("checks").cloned().unwrap_or(Value::Null),
    );
    object.insert("config_footprint".into(), paths::config_footprint());
    object.insert("side_effects".into(), side_effects());
    object.insert("fixers".into(), json!([]));
    Value::Object(object)
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
            "operator_manifest_embedded",
            serde_json::from_str::<Value>(crate::OPERATOR_JSON)
                .ok()
                .and_then(|manifest| {
                    manifest
                        .get("tool_version")
                        .and_then(Value::as_str)
                        .map(|version| version == env!("CARGO_PKG_VERSION"))
                })
                .unwrap_or(false),
            "compiled operator manifest parses and matches the crate version",
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
            "witness_ledger_untouched",
            true,
            "doctor does not read or append the ambient witness ledger",
        ),
        check(
            "config_footprint_declared",
            true,
            "benchmark manages no config or cache paths; its only state path is the witness ledger",
        ),
        check(
            "test_forced_unhealthy",
            std::env::var_os(FORCE_UNHEALTHY_ENV).is_none(),
            "test fixture can force an unhealthy doctor report",
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
        "writes_witness_ledger": false,
        "uses_network": false,
        "changes_cwd": false
    })
}

fn health_summary(payload: &Value) -> String {
    let healthy = is_healthy(payload);
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
    let status = if healthy {
        color_status("ok", true)
    } else {
        color_status("unhealthy", false)
    };
    format!("benchmark doctor health: {status}\nread_only=true\nfixers=0\nchecks={passed}/{total}")
}

fn triage_text(payload: &Value) -> String {
    let status = if is_healthy(payload) {
        color_status("healthy", true)
    } else {
        color_status("unhealthy", false)
    };
    let mut lines = vec![format!("benchmark doctor triage: {status}")];
    for action in payload
        .get("next_actions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        lines.push(format!("next: {action}"));
    }
    lines.join("\n")
}

fn color_status(status: &str, healthy: bool) -> String {
    if !color_enabled() {
        return status.to_owned();
    }
    let color = if healthy { "32" } else { "31" };
    format!("\x1b[{color}m{status}\x1b[0m")
}

fn color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some()
        || std::env::var("TERM").is_ok_and(|term| term.eq_ignore_ascii_case("dumb"))
    {
        return false;
    }

    let force = std::env::var("CLICOLOR_FORCE")
        .map(|value| !value.is_empty() && value != "0")
        .unwrap_or(false);
    force || io::stdout().is_terminal()
}

fn robot_docs_text() -> &'static str {
    "benchmark robot-docs guide\n\
contract: cmdrvl.doctor.v1\n\
agent_entrypoints:\n\
  benchmark --describe\n\
  benchmark --schema\n\
  benchmark --robot-triage\n\
  benchmark capabilities --json\n\
  benchmark robot-docs guide\n\
scoring:\n\
  benchmark <CANDIDATE> --assertions <FILE> (--key <COLUMN>[,<COLUMN>...] | --profile <FILE>) --json\n\
  benchmark <CANDIDATE> --assertions <FILE> (--key <COLUMN>[,<COLUMN>...] | --profile <FILE>) --render summary\n\
  benchmark <CANDIDATE> --assertions <FILE> (--key <COLUMN>[,<COLUMN>...] | --profile <FILE>) --lock <LOCKFILE> --json\n\
  add --no-witness to keep a run out of the ambient witness ledger\n\
witness:\n\
  benchmark witness query --json\n\
  benchmark witness last --json\n\
  benchmark witness count --json\n\
commands:\n\
  benchmark doctor --json\n\
  benchmark doctor capabilities --json\n\
  benchmark doctor triage --json\n\
  benchmark doctor health --json (alias of doctor --json)\n\
  benchmark doctor --robot-triage (alias of doctor triage)\n\
exit_codes:\n\
  0 healthy / PASS, 1 unhealthy / FAIL, 2 refusal or usage error\n\
composition:\n\
  benchmark <CANDIDATE> --assertions <FILE> (--key <COLUMN>[,<COLUMN>...] | --profile <FILE>) --json > benchmark.json\n\
  assess benchmark.json <other-artifacts> --policy <policy.yaml> --json > decision.json\n\
  pack seal benchmark.json decision.json --output evidence/\n\
read_only:\n\
  - doctor does not read stdin, candidates, assertions, lockfiles, or fixture paths\n\
  - doctor does not detect formats, open DuckDB, validate keys, verify locks, compare values, score assertions, or mint gold truth\n\
  - doctor does not create directories, write doctor artifacts, touch the witness ledger, or use the network\n\
  - no implicit ~/.cmdrvl config or cache paths are read or written by doctor; scoring runs append to the witness ledger unless --no-witness\n\
fix_mode:\n\
  - no --fix surface is implemented in this release\n\
next_steps:\n\
  - use benchmark capabilities --json for the machine-readable CLI contract\n\
  - use benchmark <candidate> --assertions <gold.jsonl> (--key <column>[,<column>...] | --profile <file>) --json for score reports"
}
