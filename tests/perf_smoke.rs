use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use benchmark::{
    Outcome,
    assertions::load_assertions,
    candidate::LoadedCandidate,
    cli::Cli,
    engine::score_candidate,
    execute,
    key_check::validate_key,
    render::{RenderMode, render_report},
    report::ReportOutcome,
};
use serde_json::Value;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
const PERF_ROW_COUNT: usize = 512;
const CANDIDATE_LOAD_MAX: Duration = Duration::from_secs(5);
const SCORING_MAX: Duration = Duration::from_secs(5);
const RENDER_MAX: Duration = Duration::from_secs(2);
const EXECUTE_TOTAL_MAX: Duration = Duration::from_secs(10);

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn temp_output_dir() -> PathBuf {
    let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "benchmark-perf-smoke-{}-{unique}",
        std::process::id()
    ))
}

struct PerfFixture {
    output_dir: PathBuf,
    candidate: PathBuf,
    assertions: PathBuf,
}

impl Drop for PerfFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.output_dir);
    }
}

fn generate_perf_fixture(row_count: usize) -> Result<PerfFixture, Box<dyn std::error::Error>> {
    let script = fixture("tests/fixtures/perf/generate_perf_smoke.sh");
    let output_dir = temp_output_dir();

    let status = Command::new("bash")
        .arg(script)
        .arg(&output_dir)
        .arg(row_count.to_string())
        .status()?;
    assert!(status.success());

    Ok(PerfFixture {
        candidate: output_dir.join("perf_candidate.csv"),
        assertions: output_dir.join("perf_assertions.jsonl"),
        output_dir,
    })
}

#[allow(non_snake_case)]
#[test]
fn BENCH_I900_perf_generator_hook_emits_deterministic_files()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = generate_perf_fixture(16)?;
    let candidate_contents = fs::read_to_string(&fixture.candidate)?;
    let assertions_contents = fs::read_to_string(&fixture.assertions)?;

    assert_eq!(candidate_contents.lines().count(), 17);
    assert_eq!(assertions_contents.lines().count(), 16);
    Ok(())
}

#[allow(non_snake_case)]
#[test]
fn BENCH_I901_perf_smoke_integrated_path_captures_phase_timings()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = generate_perf_fixture(PERF_ROW_COUNT)?;
    let assertions = load_assertions(&fixture.assertions)?;

    let candidate_load_start = Instant::now();
    let candidate = LoadedCandidate::load(&fixture.candidate)?;
    let candidate_load = candidate_load_start.elapsed();

    let key_check = validate_key(&candidate, "comp_id")?;

    let scoring_start = Instant::now();
    let report = score_candidate(
        &candidate,
        &fixture.assertions,
        &assertions,
        &key_check,
        None,
    )?;
    let scoring = scoring_start.elapsed();

    let render_start = Instant::now();
    let rendered = render_report(&report, RenderMode::Json)?;
    let rendering = render_start.elapsed();

    let execute_start = Instant::now();
    let execution = execute(Cli {
        candidate: fixture.candidate.clone(),
        assertions: fixture.assertions.clone(),
        key: "comp_id".to_owned(),
        lock: Vec::new(),
        json: true,
        render: None,
    })?;
    let execute_total = execute_start.elapsed();

    eprintln!(
        "perf_smoke rows={} candidate_load_ms={} scoring_ms={} render_ms={} execute_total_ms={}",
        PERF_ROW_COUNT,
        candidate_load.as_millis(),
        scoring.as_millis(),
        rendering.as_millis(),
        execute_total.as_millis(),
    );

    assert_eq!(report.outcome, ReportOutcome::Pass);
    assert_eq!(report.summary.total, PERF_ROW_COUNT as u64);
    assert_eq!(report.summary.passed, PERF_ROW_COUNT as u64);
    assert_eq!(report.summary.failed, 0);
    assert_eq!(report.summary.skipped, 0);

    let report_json: Value = serde_json::from_str(&rendered)?;
    assert_eq!(report_json["outcome"], "PASS");
    assert_eq!(report_json["summary"]["passed"], PERF_ROW_COUNT as u64);

    assert_eq!(execution.outcome, Outcome::Pass);
    let execution_json: Value = serde_json::from_str(&execution.stdout)?;
    assert_eq!(execution_json["outcome"], "PASS");
    assert_eq!(execution_json["summary"]["passed"], PERF_ROW_COUNT as u64);

    assert!(
        candidate_load <= CANDIDATE_LOAD_MAX,
        "candidate load exceeded guardrail: {:?} > {:?}",
        candidate_load,
        CANDIDATE_LOAD_MAX
    );
    assert!(
        scoring <= SCORING_MAX,
        "scoring exceeded guardrail: {:?} > {:?}",
        scoring,
        SCORING_MAX
    );
    assert!(
        rendering <= RENDER_MAX,
        "rendering exceeded guardrail: {:?} > {:?}",
        rendering,
        RENDER_MAX
    );
    assert!(
        execute_total <= EXECUTE_TOTAL_MAX,
        "execute total exceeded guardrail: {:?} > {:?}",
        execute_total,
        EXECUTE_TOTAL_MAX
    );

    Ok(())
}
