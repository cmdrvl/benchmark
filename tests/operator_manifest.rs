//! benchmark's operator manifest: `operator.json`, `--describe`, `capabilities --json`.

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::{Value, json};
use spine_rules::contract::{OperatorContract, assert_version_parity, cargo_package_version};

const OPERATOR_JSON: &str = include_str!("../operator.json");
const CARGO_TOML: &str = include_str!("../Cargo.toml");

fn stdout_of(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_benchmark"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .expect("benchmark should run");
    assert!(
        output.status.success(),
        "benchmark {args:?} should exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout should be UTF-8")
}

#[test]
fn describe_is_byte_identical_to_operator_json() {
    assert_eq!(
        stdout_of(&["--describe"]).trim_end(),
        OPERATOR_JSON.trim_end()
    );
}

#[test]
fn capabilities_json_is_the_same_document_as_describe() {
    assert_eq!(
        stdout_of(&["capabilities", "--json"]),
        stdout_of(&["--describe"])
    );
}

#[test]
fn operator_json_version_equals_cargo_package_version() {
    assert_eq!(
        cargo_package_version(CARGO_TOML).as_deref(),
        Some(env!("CARGO_PKG_VERSION"))
    );
    assert_version_parity(OPERATOR_JSON, CARGO_TOML);
    let manifest: Value = serde_json::from_str(OPERATOR_JSON).expect("operator.json is JSON");
    assert_eq!(manifest["tool_version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn operator_json_is_valid_operator_v1() {
    let contract = OperatorContract::parse(OPERATOR_JSON).expect("operator.json parses");
    assert_eq!(contract.validate(), Vec::<String>::new());
    assert_eq!(contract.name, "benchmark");
}

/// Every refusal code the source can emit is declared, and nothing else is.
#[test]
fn declared_refusals_equal_codes_in_source() {
    let manifest: Value = serde_json::from_str(OPERATOR_JSON).expect("operator.json is JSON");
    let declared: BTreeSet<String> = manifest["refusals"]
        .as_array()
        .expect("refusals array")
        .iter()
        .filter_map(|refusal| refusal["code"].as_str().map(str::to_owned))
        .collect();

    let mut in_source = BTreeSet::new();
    for file in [
        "src/assertions.rs",
        "src/key_check.rs",
        "src/lock_check.rs",
        "src/lib.rs",
        "src/refusal.rs",
    ] {
        let text = std::fs::read_to_string(format!("{}/{file}", env!("CARGO_MANIFEST_DIR")))
            .expect("source file readable");
        let production = text.split("#[cfg(test)]").next().unwrap_or_default();
        for chunk in production.split('"').skip(1).step_by(2) {
            if chunk.starts_with("E_") && chunk.chars().all(|c| c.is_ascii_uppercase() || c == '_')
            {
                in_source.insert(chunk.to_owned());
            }
        }
    }
    assert_eq!(declared, in_source);
}

/// Planted negatives: the validator the manifest is held to must reject drift.
#[test]
fn planted_bad_manifests_are_rejected() {
    let base: Value = serde_json::from_str(OPERATOR_JSON).expect("operator.json is JSON");
    let violations = |mutate: &dyn Fn(&mut Value)| {
        let mut doc = base.clone();
        mutate(&mut doc);
        OperatorContract::parse(&doc.to_string())
            .expect("mutated manifest still parses")
            .validate()
    };

    assert!(
        violations(&|doc| {
            doc["commands"]
                .as_array_mut()
                .expect("commands")
                .retain(|command| command["name"] != "robot-docs");
        })
        .iter()
        .any(|violation| violation.contains("robot-docs")),
        "a manifest without `robot-docs` must be rejected"
    );
    assert!(
        !violations(&|doc| {
            doc["exit_codes"]["7"] = json!({"meaning": "bogus", "domain": "error"});
        })
        .is_empty(),
        "an undeclared exit code must be rejected"
    );
    assert!(
        !violations(&|doc| {
            doc["refusals"][0]["code"] = json!("NOT_PREFIXED");
        })
        .is_empty(),
        "a refusal code without the E_ prefix must be rejected"
    );

    let stale = OPERATOR_JSON.replace(env!("CARGO_PKG_VERSION"), "0.0.1");
    let result = std::panic::catch_unwind(|| assert_version_parity(&stale, CARGO_TOML));
    assert!(
        result.is_err(),
        "a stale manifest version must fail the parity check"
    );
}
