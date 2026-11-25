//! Ambient witness ledger, exercised through the real binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

static COUNTER: AtomicU64 = AtomicU64::new(0);

const CANDIDATE: &str = "tests/fixtures/candidates/smoke/bench_i001_candidate.csv";
const GOLD: &str = "tests/fixtures/assertions/smoke/bench_i001_gold.jsonl";
const MIXED: &str = "tests/fixtures/candidates/smoke/bench_mixed.csv";
const MIXED_GOLD: &str = "tests/fixtures/assertions/smoke/bench_mixed_gold.jsonl";

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "benchmark-witness-cli-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

fn benchmark(ledger: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_benchmark"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("EPISTEMIC_WITNESS", ledger)
        .env("SOURCE_DATE_EPOCH", "1700000000")
        .args(args)
        .output()
        .expect("benchmark should run")
}

fn score(candidate: &str, gold: &str, extra: &[&str]) -> Vec<String> {
    let mut args = vec![
        candidate,
        "--assertions",
        gold,
        "--key",
        "comp_id",
        "--json",
    ];
    args.extend_from_slice(extra);
    args.into_iter().map(str::to_owned).collect()
}

fn run_score(ledger: &Path, candidate: &str, gold: &str, extra: &[&str]) -> Output {
    let args = score(candidate, gold, extra);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    benchmark(ledger, &refs)
}

fn ledger_records(ledger: &Path) -> Vec<Value> {
    fs::read_to_string(ledger)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

#[test]
fn scoring_run_appends_one_record_with_hashed_inputs() {
    let ledger = scratch().join("witness.jsonl");
    let output = run_score(&ledger, CANDIDATE, GOLD, &[]);
    assert_eq!(output.status.code(), Some(0));

    let records = ledger_records(&ledger);
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record["tool"], "benchmark");
    assert_eq!(record["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(record["outcome"], "PASS");
    assert_eq!(record["exit_code"], 0);
    assert_eq!(record["params"]["key"], "comp_id");
    assert_eq!(record["inputs"].as_array().map(Vec::len), Some(2));
    for input in record["inputs"].as_array().into_iter().flatten() {
        assert!(
            input["hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with("blake3:"))
        );
    }
    // The candidate hash in the record is the BLAKE3 of the exact bytes scored.
    let bytes = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(CANDIDATE))
        .expect("fixture should be readable");
    assert_eq!(
        record["inputs"][0]["hash"],
        format!("blake3:{}", blake3::hash(&bytes).to_hex())
    );
    assert!(
        record["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("blake3:"))
    );
}

#[test]
fn record_timestamp_honors_source_date_epoch() {
    let ledger = scratch().join("witness.jsonl");
    run_score(&ledger, CANDIDATE, GOLD, &[]);
    assert_eq!(ledger_records(&ledger)[0]["ts"], "2023-11-14T22:13:20Z");
}

#[test]
fn no_witness_suppresses_the_ledger_and_its_absence_is_the_difference() {
    let dir = scratch();
    let suppressed = dir.join("suppressed.jsonl");
    let recorded = dir.join("recorded.jsonl");

    let with_flag = run_score(&suppressed, CANDIDATE, GOLD, &["--no-witness"]);
    let without_flag = run_score(&recorded, CANDIDATE, GOLD, &[]);

    assert_eq!(with_flag.status.code(), without_flag.status.code());
    assert!(
        !suppressed.exists(),
        "--no-witness must not create a ledger"
    );
    assert!(
        recorded.exists(),
        "the same run without the flag must record"
    );
}

#[test]
fn failing_and_refused_runs_are_recorded_with_their_exit_codes() {
    let ledger = scratch().join("witness.jsonl");

    let failed = run_score(&ledger, MIXED, MIXED_GOLD, &[]);
    assert_eq!(failed.status.code(), Some(1));
    let refused = run_score(&ledger, "/nonexistent/candidate.csv", GOLD, &[]);
    assert_eq!(refused.status.code(), Some(2));

    let records = ledger_records(&ledger);
    assert_eq!(records.len(), 2);
    assert_eq!(
        (
            records[0]["outcome"].as_str(),
            records[0]["exit_code"].as_u64()
        ),
        (Some("FAIL"), Some(1))
    );
    assert_eq!(
        (
            records[1]["outcome"].as_str(),
            records[1]["exit_code"].as_u64()
        ),
        (Some("REFUSAL"), Some(2))
    );
    // The unreadable candidate is not hashed; the readable assertion file still is.
    assert_eq!(records[1]["inputs"].as_array().map(Vec::len), Some(1));
}

#[test]
fn witness_write_failure_never_changes_the_exit_code() {
    let dir = scratch();
    let baseline = run_score(&dir.join("ok.jsonl"), CANDIDATE, GOLD, &[]);
    let impossible = Path::new("/dev/null/impossible/witness.jsonl");
    let broken = run_score(impossible, CANDIDATE, GOLD, &[]);

    assert_eq!(broken.status.code(), baseline.status.code());
    assert_eq!(
        broken.stdout, baseline.stdout,
        "report output is unaffected"
    );
    assert!(
        String::from_utf8_lossy(&broken.stderr).contains("witness"),
        "a failed write is reported on stderr"
    );

    let failing = run_score(impossible, MIXED, MIXED_GOLD, &[]);
    assert_eq!(failing.status.code(), Some(1), "FAIL stays exit 1");
}

#[test]
fn query_last_and_count_read_the_ledger_back() {
    let ledger = scratch().join("witness.jsonl");
    run_score(&ledger, CANDIDATE, GOLD, &[]);
    run_score(&ledger, MIXED, MIXED_GOLD, &[]);

    let count = benchmark(&ledger, &["witness", "count", "--json"]);
    assert_eq!(count.status.code(), Some(0));
    let count: Value = serde_json::from_slice(&count.stdout).expect("count JSON");
    assert_eq!(count["count"], 2);

    let fails = benchmark(
        &ledger,
        &["witness", "query", "--outcome", "FAIL", "--json"],
    );
    let fails: Value = serde_json::from_slice(&fails.stdout).expect("query JSON");
    assert_eq!(fails.as_array().map(Vec::len), Some(1));
    assert_eq!(fails[0]["outcome"], "FAIL");

    let last = benchmark(&ledger, &["witness", "last", "--json"]);
    let last: Value = serde_json::from_slice(&last.stdout).expect("last JSON");
    assert_eq!(last["outcome"], "FAIL");
}

#[test]
fn empty_results_exit_zero_and_print_an_empty_array() {
    let ledger = scratch().join("never-written.jsonl");

    let query = benchmark(&ledger, &["witness", "query", "--json"]);
    assert_eq!(query.status.code(), Some(0));
    assert_eq!(
        serde_json::from_slice::<Value>(&query.stdout).expect("JSON"),
        serde_json::json!([])
    );

    let count = benchmark(&ledger, &["witness", "count", "--json"]);
    assert_eq!(count.status.code(), Some(0));
    assert_eq!(
        serde_json::from_slice::<Value>(&count.stdout).expect("JSON")["count"],
        0
    );

    let last = benchmark(&ledger, &["witness", "last", "--json"]);
    assert_eq!(last.status.code(), Some(0));
}

#[test]
fn read_only_commands_do_not_write_the_ledger() {
    let ledger = scratch().join("witness.jsonl");
    for args in [
        vec!["doctor", "--json"],
        vec!["capabilities", "--json"],
        vec!["--describe"],
        vec!["--schema"],
        vec!["witness", "count", "--json"],
    ] {
        benchmark(&ledger, &args);
    }
    assert!(!ledger.exists());
}
