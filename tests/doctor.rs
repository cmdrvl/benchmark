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

    assert_eq!(payload["schema"], "benchmark.doctor.health.v1");
    assert_eq!(payload["contract"], "cmdrvl.read_only_doctor.v1");
    assert_eq!(payload["tool"], "benchmark");
    assert_eq!(payload["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(payload["report_version"], "benchmark.v0");
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["read_only"], true);
    assert_eq!(payload["summary"]["checks_failed"], 0);
    assert_eq!(payload["observed_paths"]["candidate"], Value::Null);
    assert_eq!(payload["side_effects"]["opens_duckdb_connection"], false);
    assert_eq!(payload["side_effects"]["scores_assertions"], false);
    assert_eq!(payload["side_effects"]["promotes_gold_truth"], false);
    assert_all_side_effects_false(side_effects(&payload)?)?;
    assert!(payload["fixers"].as_array().is_some_and(Vec::is_empty));
    assert_doctor_artifacts_absent();
    Ok(())
}

#[test]
fn doctor_capabilities_json_has_no_fixers_or_side_effects() -> Result<(), Box<dyn std::error::Error>>
{
    let output = benchmark_cmd()
        .args(["doctor", "capabilities", "--json"])
        .output()?;
    let payload = parse_stdout_json(&output)?;

    assert_eq!(payload["schema"], "benchmark.doctor.capabilities.v1");
    assert_eq!(payload["contract"], "cmdrvl.read_only_doctor.v1");
    assert_eq!(payload["read_only"], true);
    assert_all_side_effects_false(side_effects(&payload)?)?;
    assert!(payload["fixers"].as_array().is_some_and(Vec::is_empty));
    assert!(
        payload["commands"]
            .as_array()
            .map(|commands| {
                commands.iter().any(|command| {
                    command["name"] == "robot-triage"
                        && command["usage"] == "benchmark doctor --robot-triage"
                })
            })
            .unwrap_or(false)
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

    assert_eq!(payload["schema"], "benchmark.doctor.triage.v1");
    assert_eq!(payload["contract"], "cmdrvl.read_only_doctor.v1");
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["score"], 100);
    assert_eq!(payload["read_only"], true);
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
    assert!(stdout.contains("cmdrvl.read_only_doctor.v1"));
    assert!(stdout.contains("benchmark doctor health --json"));
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
