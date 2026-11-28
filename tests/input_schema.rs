use std::{fs, process::Command};

use benchmark::{INPUT_SCHEMA_JSON, assertions::load_assertions, cli::Cli, execute};
use clap::Parser;
use jsonschema::validator_for;
use serde_json::{Value, json};
use tempfile::tempdir;

fn schema() -> Value {
    serde_json::from_str(INPUT_SCHEMA_JSON).expect("assertion schema is JSON")
}

#[test]
fn input_schema_flag_emits_a_valid_schema_for_the_minimal_record()
-> Result<(), Box<dyn std::error::Error>> {
    let execution = execute(Cli::parse_from(["benchmark", "--input-schema"]))?;
    assert_eq!(execution.exit_code(), 0);

    let emitted: Value = serde_json::from_str(&execution.stdout)?;
    assert_eq!(emitted, schema());
    let validator = validator_for(&emitted)?;
    let minimal = json!({
        "entity": "comp_4",
        "field": "u8:cap_rate",
        "expected": "6.76%"
    });
    assert!(validator.is_valid(&minimal));
    assert_eq!(emitted["required"], json!(["entity", "field", "expected"]));
    Ok(())
}

#[test]
fn documented_example_loads_through_the_loader_and_scores_a_candidate()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempdir()?;
    let candidate = root.path().join("candidate.csv");
    let assertions = root.path().join("assertions.jsonl");
    fs::write(&candidate, "comp_id,actual\ncomp_4,6.76%\n")?;
    let example = "{\"entity\":\"comp_4\",\"field\":\"actual\",\"expected\":\"6.76%\"}\n";
    fs::write(&assertions, example)?;

    let loaded = load_assertions(&assertions)?;
    assert_eq!(loaded.len(), 1);

    let output = Command::new(env!("CARGO_BIN_EXE_benchmark"))
        .args([
            candidate.to_str().unwrap(),
            "--assertions",
            assertions.to_str().unwrap(),
            "--key",
            "comp_id",
            "--json",
            "--no-witness",
        ])
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["outcome"], "PASS");
    assert_eq!(report["summary"]["passed"], 1);
    Ok(())
}

#[test]
fn invalid_assertion_example_is_rejected_by_schema_loader_and_cli()
-> Result<(), Box<dyn std::error::Error>> {
    let invalid = json!({
        "entity": "comp_4",
        "field": "actual",
        "expected": "6.76%",
        "compare_as": "string",
        "tolerance": 0.01
    });
    let validator = validator_for(&schema())?;
    assert!(!validator.is_valid(&invalid));

    let root = tempdir()?;
    let candidate = root.path().join("candidate.csv");
    let assertions = root.path().join("invalid.jsonl");
    fs::write(&candidate, "comp_id,actual\ncomp_4,6.76%\n")?;
    fs::write(
        &assertions,
        format!("{}\n", serde_json::to_string(&invalid)?),
    )?;
    assert!(load_assertions(&assertions).is_err());

    let output = Command::new(env!("CARGO_BIN_EXE_benchmark"))
        .args([
            candidate.to_str().unwrap(),
            "--assertions",
            assertions.to_str().unwrap(),
            "--key",
            "comp_id",
            "--json",
            "--no-witness",
        ])
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    let refusal: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(refusal["refusal"]["code"], "E_BAD_ASSERTIONS");
    Ok(())
}
