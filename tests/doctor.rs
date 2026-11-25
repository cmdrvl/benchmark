use std::{io, path::Path, process::Command};

use serde_json::Value;

fn benchmark_cmd() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_benchmark"));
    command.current_dir(env!("CARGO_MANIFEST_DIR"));
    command
}

fn manifest_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn missing_json_field(name: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("missing JSON field {name}"),
    )
}

fn parse_stdout_json(output: &std::process::Output) -> Result<Value, Box<dyn std::error::Error>> {
    assert!(
        output.status.success(),
        "expected success, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn side_effects(payload: &Value) -> Result<&Value, Box<dyn std::error::Error>> {
    payload
        .get("side_effects")
        .ok_or_else(|| missing_json_field("side_effects").into())
}

fn assert_all_side_effects_false(side_effects: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let object = side_effects
        .as_object()
        .ok_or_else(|| missing_json_field("side_effects object"))?;
    assert!(!object.is_empty());
    for (name, value) in object {
        assert_eq!(value, false, "side effect {name} should be false");
    }
    Ok(())
}

fn assert_doctor_artifacts_absent() {
    assert!(!manifest_root().join(".doctor").exists());
}

#[test]
fn doctor_health_json_is_read_only() -> Result<(), Box<dyn std::error::Error>> {
    let output = benchmark_cmd()
        .args(["doctor", "health", "--json"])
        .output()?;
    let payload = parse_stdout_json(&output)?;

    assert_eq!(payload["schema_version"], "cmdrvl.doctor.v1");
    assert_eq!(payload["mode"], "health");
    assert_eq!(payload["healthy"], true);
    assert_eq!(payload["tool"], "benchmark");
    assert_eq!(payload["tool_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(payload["report_version"], "benchmark.v0");
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["read_only"], true);
    assert_eq!(payload["summary"]["checks_failed"], 0);
    assert_eq!(payload["observed_paths"]["candidate"], Value::Null);
    assert_eq!(payload["config_footprint"]["canonical_root"], "~/.cmdrvl");
    assert_eq!(
        payload["config_footprint"]["legacy_migration_required"],
        false
    );
    assert!(
        payload["config_footprint"]["managed_config_paths"]
            .as_array()
            .is_some_and(Vec::is_empty)
    );
    assert_eq!(payload["side_effects"]["opens_duckdb_connection"], false);
    assert_eq!(payload["side_effects"]["scores_assertions"], false);
    assert_eq!(payload["side_effects"]["reads_config_files"], false);
    assert_eq!(payload["side_effects"]["writes_migration_logs"], false);
    assert_eq!(payload["side_effects"]["promotes_gold_truth"], false);
    assert_all_side_effects_false(side_effects(&payload)?)?;
    assert!(payload["fixers"].as_array().is_some_and(Vec::is_empty));
    assert_doctor_artifacts_absent();
    Ok(())
}

#[test]
fn unhealthy_doctor_state_returns_exit_one() -> Result<(), Box<dyn std::error::Error>> {
    let output = benchmark_cmd()
        .args(["doctor", "--json"])
        .env("BENCHMARK_DOCTOR_FORCE_UNHEALTHY", "1")
        .output()?;

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let payload: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(payload["healthy"], false);
    assert_eq!(payload["findings"].as_array().map(Vec::len), Some(14));
    assert!(payload["findings"].as_array().is_some_and(|findings| {
        findings.iter().any(|finding| {
            finding["id"] == "test_forced_unhealthy" && finding["severity"] == "error"
        })
    }));
    Ok(())
}

#[test]
fn no_color_and_non_tty_guard_forced_ansi() -> Result<(), Box<dyn std::error::Error>> {
    let forced = benchmark_cmd()
        .args(["doctor"])
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR")
        .env_remove("TERM")
        .output()?;
    assert!(String::from_utf8_lossy(&forced.stdout).contains("\x1b["));

    let no_color = benchmark_cmd()
        .args(["doctor"])
        .env("CLICOLOR_FORCE", "1")
        .env("NO_COLOR", "1")
        .env_remove("TERM")
        .output()?;
    assert!(!String::from_utf8_lossy(&no_color.stdout).contains("\x1b["));

    let dumb = benchmark_cmd()
        .args(["doctor"])
        .env("CLICOLOR_FORCE", "1")
        .env_remove("NO_COLOR")
        .env("TERM", "dumb")
        .output()?;
    assert!(!String::from_utf8_lossy(&dumb.stdout).contains("\x1b["));

    let non_tty = benchmark_cmd()
        .args(["doctor"])
        .env_remove("CLICOLOR_FORCE")
        .env_remove("NO_COLOR")
        .env_remove("TERM")
        .output()?;
    assert!(!String::from_utf8_lossy(&non_tty.stdout).contains("\x1b["));
    Ok(())
}

#[test]
fn doctor_capabilities_json_has_no_fixers_or_side_effects() -> Result<(), Box<dyn std::error::Error>>
{
    let output = benchmark_cmd()
        .args(["doctor", "capabilities", "--json"])
        .output()?;
    let payload = parse_stdout_json(&output)?;

    assert_eq!(payload["schema_version"], "cmdrvl.doctor.v1");
    assert_eq!(payload["mode"], "capabilities");
    assert_eq!(payload["healthy"], true);

    // The embedded contract is exactly the top-level `capabilities --json` document.
    let top_level = benchmark_cmd().args(["capabilities", "--json"]).output()?;
    let top_level = parse_stdout_json(&top_level)?;
    assert_eq!(payload["contract"], top_level);
    assert_eq!(payload["contract"]["schema_version"], "operator.v1");
    assert_eq!(
        payload["contract"]["composition"]["role"],
        "gold_set_scorer"
    );
    assert_eq!(payload["contract"]["side_effects"]["network"], false);
    assert_eq!(
        payload["contract"]["capabilities"]["agent_surfaces"]["robot_triage"],
        "benchmark --robot-triage"
    );
    assert_doctor_artifacts_absent();
    Ok(())
}

#[test]
fn doctor_robot_triage_json_is_machine_readable() -> Result<(), Box<dyn std::error::Error>> {
    let output = benchmark_cmd()
        .args(["doctor", "--robot-triage"])
        .output()?;
    let payload = parse_stdout_json(&output)?;

    assert_eq!(payload["schema_version"], "cmdrvl.doctor.v1");
    assert_eq!(payload["mode"], "triage");
    assert_eq!(payload["healthy"], true);
    assert!(
        payload["next_actions"]
            .as_array()
            .is_some_and(|actions| !actions.is_empty())
    );
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["score"], 100);
    assert_eq!(payload["read_only"], true);
    assert_eq!(payload["config_footprint"]["canonical_root"], "~/.cmdrvl");
    assert_all_side_effects_false(side_effects(&payload)?)?;
    assert_doctor_artifacts_absent();
    Ok(())
}

#[test]
fn doctor_robot_docs_is_plain_text_and_read_only() -> Result<(), Box<dyn std::error::Error>> {
    let output = benchmark_cmd().args(["doctor", "robot-docs"]).output()?;

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("cmdrvl.doctor.v1"));
    assert!(stdout.contains("benchmark --robot-triage"));
    assert!(stdout.contains("benchmark capabilities --json"));
    assert!(stdout.contains("benchmark robot-docs guide"));
    assert!(stdout.contains("benchmark doctor health --json"));
    assert!(stdout.contains(
        "assess benchmark.json <other-artifacts> --policy <policy.yaml> --json > decision.json"
    ));
    assert!(stdout.contains("pack seal benchmark.json decision.json --output evidence/"));
    assert!(stdout.contains("no implicit ~/.cmdrvl config"));
    assert!(stdout.contains("no --fix surface"));
    assert_doctor_artifacts_absent();
    Ok(())
}

#[test]
fn doctor_help_is_available() -> Result<(), Box<dyn std::error::Error>> {
    let output = benchmark_cmd().args(["doctor", "--help"]).output()?;

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("health"));
    assert!(stdout.contains("capabilities"));
    assert!(stdout.contains("robot-docs"));
    assert!(stdout.contains("--robot-triage"));
    Ok(())
}

#[test]
fn doctor_fix_is_not_available() -> Result<(), Box<dyn std::error::Error>> {
    let output = benchmark_cmd().args(["doctor", "--fix"]).output()?;

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(stderr.contains("--fix"));
    assert_doctor_artifacts_absent();
    Ok(())
}
