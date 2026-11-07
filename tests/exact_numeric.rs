//! Exact native scoring on declared synthetic scalars, not domain accuracy proof.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::Value;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

struct CaseDir(PathBuf);
impl CaseDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "benchmark-exact-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create test directory");
        Self(path)
    }
    fn run(&self, extension: &str, candidate: &str, assertion: &str) -> (i32, Value) {
        let input = self.0.join(format!("candidate.{extension}"));
        fs::write(&input, candidate).expect("write candidate");
        self.run_path(&input, assertion)
    }
    fn run_path(&self, input: &std::path::Path, assertion: &str) -> (i32, Value) {
        let labels = self.0.join("assertions.jsonl");
        fs::write(&labels, format!("{assertion}\n")).expect("write assertions");
        let result = Command::new(env!("CARGO_BIN_EXE_benchmark"))
            .args([
                input.to_str().unwrap(),
                "--assertions",
                labels.to_str().unwrap(),
                "--key",
                "row_id",
                "--json",
            ])
            .output()
            .expect("run native benchmark");
        let report = serde_json::from_slice(&result.stdout).unwrap_or_else(|error| {
            panic!(
                "invalid native output: {error}: {} stderr:{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            )
        });
        (result.status.code().unwrap(), report)
    }
}
impl Drop for CaseDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assertion(expected: &str, mode: &str, tolerance: Option<&str>) -> String {
    let tail = tolerance
        .map(|token| format!(",\"tolerance\":{token}"))
        .unwrap_or_default();
    format!(
        r#"{{"entity":"A","field":"value","expected":{expected},"compare_as":"{mode}"{tail}}}"#,
        expected = serde_json::to_string(expected).unwrap()
    )
}

#[test]
fn distinct_decimals_survive_all_text_formats_and_exact_number_comparison() {
    let dir = CaseDir::new();
    for (expected, actual) in [
        ("9007199254740992", "9007199254740993"),
        ("12.5", "12.5000000000000001"),
        ("0", "1e-400"),
    ] {
        let labels = assertion(expected, "number", Some("0"));
        for (extension, input) in [
            ("csv", format!("row_id,value\nA,{actual}\n")),
            ("json", format!(r#"[{{"row_id":"A","value":{actual}}}]"#)),
            ("jsonl", format!(r#"{{"row_id":"A","value":{actual}}}"#)),
            ("json", format!(r#"[{{"row_id":"A","value":"{actual}"}}]"#)),
        ] {
            let (code, report) = dir.run(extension, &input, &labels);
            assert_eq!(code, 1, "{extension} {actual}: {report}");
            assert_eq!(report["failures"][0]["actual"], actual);
            assert_eq!(report["summary"]["failed"], 1);
        }
    }
}

#[test]
fn source_string_lexemes_are_not_inferred_as_numbers() {
    let dir = CaseDir::new();
    for (expected, actual) in [
        ("12.5", "12.5000000000000001"),
        ("123", "00123"),
        ("1.23", "1.2300"),
        ("1000", "1E+3"),
    ] {
        for (extension, input) in [
            ("csv", format!("row_id,value\nA,{actual}\n")),
            (
                "json",
                format!(r#"[{{"row_id":"A","value":{actual}}}]"#).replace(":00123", ":\"00123\""),
            ),
        ] {
            let (code, report) = dir.run(extension, &input, &assertion(expected, "string", None));
            assert_eq!(code, 1, "{extension} {actual}: {report}");
            assert_eq!(report["failures"][0]["actual"], actual);
        }
    }
}

#[test]
fn mathematically_equal_decimal_forms_and_exact_tolerance_boundaries_match() {
    let dir = CaseDir::new();
    for (expected, actual, mode, tolerance, code) in [
        ("12.50", "12.5", "number", "0", 0),
        ("-0.125", "-1.25e-1", "number", "0", 0),
        ("1000", "+1E3", "number", "0", 0),
        ("-5.0%", "-5.0000000000000000001%", "percent", "0", 1),
        (
            "0",
            "0.100000000000000000001",
            "number",
            "0.100000000000000000001",
            0,
        ),
        (
            "0",
            "0.100000000000000000002",
            "number",
            "0.100000000000000000001",
            1,
        ),
        (
            "0%",
            "0.100000000000000000002%",
            "percent",
            "0.100000000000000000001",
            1,
        ),
        ("0", "1e-400", "number", "1e-400", 0),
        ("0", "1.0000000000000001e-400", "number", "1e-400", 1),
    ] {
        let (actual_code, report) = dir.run(
            "csv",
            &format!("row_id,value\nA,{actual}\n"),
            &assertion(expected, mode, Some(tolerance)),
        );
        assert_eq!(
            actual_code, code,
            "{expected} {actual} tolerance={tolerance}: {report}"
        );
        if code == 1 {
            assert_eq!(report["failures"][0]["tolerance"].to_string(), tolerance);
            assert_eq!(report["failures"][0]["actual"], actual);
        }
    }
}

#[test]
fn invalid_expected_numbers_and_tolerances_refuse_without_numeric_fallback() {
    let dir = CaseDir::new();
    for expected in ["NaN", "inf", "1_000", "1e1025", "10e9223372036854775807"] {
        let (code, report) = dir.run(
            "csv",
            "row_id,value\nA,1\n",
            &assertion(expected, "number", Some("0")),
        );
        assert_eq!(code, 2, "{report}");
        assert_eq!(report["refusal"]["code"], "E_BAD_ASSERTIONS");
    }
    for tolerance in ["-0.1", "1e1025", "\"0.1\""] {
        let (code, report) = dir.run(
            "csv",
            "row_id,value\nA,1\n",
            &assertion("1", "number", Some(tolerance)),
        );
        assert_eq!(code, 2, "{report}");
        assert_eq!(report["refusal"]["code"], "E_BAD_ASSERTIONS");
    }
}

#[test]
fn parquet_stored_integer_and_decimal_values_are_compared_without_f64() {
    let dir = CaseDir::new();
    let input = dir.0.join("candidate.parquet");
    let connection = duckdb::Connection::open_in_memory().expect("native parquet writer");
    connection
        .execute_batch(&format!(
            "COPY (SELECT 'A' AS row_id, 9007199254740993::BIGINT AS integer_value, \
         12.500000000000000001::DECIMAL(38,18) AS decimal_value) TO '{}' (FORMAT PARQUET);",
            input.to_str().unwrap().replace('\'', "''")
        ))
        .expect("write stored exact scalars");
    for (field, expected, code) in [
        ("integer_value", "9007199254740993", 0),
        ("integer_value", "9007199254740992", 1),
        ("decimal_value", "12.500000000000000001", 0),
        ("decimal_value", "12.5", 1),
    ] {
        let labels = assertion(expected, "number", Some("0"))
            .replace("\"field\":\"value\"", &format!("\"field\":\"{field}\""));
        let (actual_code, report) = dir.run_path(&input, &labels);
        assert_eq!(actual_code, code, "{field} {expected}: {report}");
    }
}

#[test]
fn exponent_tolerance_is_preserved_in_human_output() {
    let dir = CaseDir::new();
    let (code, _) = dir.run(
        "csv",
        "row_id,value\nA,1\n",
        &assertion("0", "number", Some("1.0e-100")),
    );
    assert_eq!(code, 1);
    let result = Command::new(env!("CARGO_BIN_EXE_benchmark"))
        .arg(dir.0.join("candidate.csv"))
        .arg("--assertions")
        .arg(dir.0.join("assertions.jsonl"))
        .args(["--key", "row_id"])
        .output()
        .expect("run human output");
    assert_eq!(result.status.code(), Some(1));
    assert!(
        String::from_utf8(result.stdout)
            .unwrap()
            .contains("tolerance=1.0e-100")
    );
}

#[test]
fn json_scalar_schema_missingness_and_key_identity_remain_explicit() {
    let dir = CaseDir::new();
    for input in [
        r#"[{"row_id":"A","value":{"amount":1}}]"#,
        r#"[{"row_id":"A","value":[1]}]"#,
        r#"[{"row_id":"A","value":1,"value":2}]"#,
    ] {
        let (code, report) = dir.run("json", input, &assertion("1", "number", Some("0")));
        assert_eq!(code, 2, "{report}");
    }
    let (code, report) = dir.run(
        "json",
        r#"[{"row_id":"A","value":null}]"#,
        &assertion("0", "number", Some("0")),
    );
    assert_eq!(code, 1, "{report}");
    assert_eq!(report["summary"]["failed"], 1);
    let (code, report) = dir.run(
        "csv",
        "row_id,value\n01,1\n1,1\n",
        &assertion("1", "number", Some("0")),
    );
    assert_eq!(
        code, 1,
        "distinct text keys must not become a duplicate refusal: {report}"
    );
    assert_eq!(report["summary"]["skipped"], 1);
}
