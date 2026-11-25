use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use benchmark::lock_check::{LockCheckError, verify_candidate};

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

#[allow(non_snake_case)]
#[test]
fn BENCH_I013_lock_fixture_verifies_smoke_candidate() -> Result<(), Box<dyn std::error::Error>> {
    let candidate = fixture("tests/fixtures/candidates/smoke/bench_i001_candidate.csv");
    let lockfile = fixture("tests/fixtures/locks/smoke/bench_i010_candidate.lock.json");

    let verification = verify_candidate(&candidate, std::slice::from_ref(&lockfile))?;

    assert_eq!(verification.lockfiles, vec![lockfile.clone()]);
    assert_eq!(verification.matched_lockfile, lockfile);
    assert_eq!(verification.verified_member, "bench_i001_candidate.csv");
    assert!(verification.candidate_hash.starts_with("sha256:"));
    Ok(())
}

#[allow(non_snake_case)]
#[test]
fn BENCH_I009_lock_drift_fixture_refuses_before_scoring() -> Result<(), Box<dyn std::error::Error>>
{
    let candidate = fixture("tests/fixtures/candidates/smoke/bench_i001_candidate.csv");
    let lockfile = fixture("tests/fixtures/locks/refusal/bench_drift.lock.json");

    let error = verify_candidate(&candidate, std::slice::from_ref(&lockfile)).unwrap_err();
    assert_eq!(error.refusal_code(), "E_INPUT_DRIFT");
    assert!(matches!(error, LockCheckError::InputDrift { .. }));
    Ok(())
}

#[allow(non_snake_case)]
#[test]
fn BENCH_I010_lock_non_member_fixture_refuses_before_scoring()
-> Result<(), Box<dyn std::error::Error>> {
    let candidate = fixture("tests/fixtures/candidates/smoke/bench_i001_candidate.csv");
    let lockfile = fixture("tests/fixtures/locks/refusal/bench_non_member.lock.json");

    let error = verify_candidate(&candidate, std::slice::from_ref(&lockfile)).unwrap_err();
    assert_eq!(error.refusal_code(), "E_INPUT_NOT_LOCKED");
    assert!(matches!(error, LockCheckError::InputNotLocked { .. }));
    Ok(())
}

#[test]
fn benchmark_lock_verifies_committed_real_lockfiles_for_sha256_and_blake3()
-> Result<(), Box<dyn std::error::Error>> {
    let candidate = fixture("tests/fixtures/candidates/smoke/bench_i001_candidate.csv");
    for algorithm in ["sha256", "blake3"] {
        let lockfile = fixture(&format!(
            "tests/fixtures/locks/real/benchmark-real-lock-{algorithm}.lock.json"
        ));
        let verification = verify_candidate(&candidate, std::slice::from_ref(&lockfile))?;
        assert!(
            verification
                .candidate_hash
                .starts_with(&format!("{algorithm}:"))
        );

        let output = Command::new(env!("CARGO_BIN_EXE_benchmark"))
            .arg(&candidate)
            .args([
                "--assertions",
                "tests/fixtures/assertions/smoke/bench_i001_gold.jsonl",
                "--key",
                "comp_id",
                "--lock",
            ])
            .arg(&lockfile)
            .args(["--json", "--no-witness"])
            .output()?;
        assert!(
            output.status.success(),
            "benchmark --lock {algorithm} failed: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(report["outcome"], "PASS");
        assert!(
            report["input_verification"]["candidate_hash"]
                .as_str()
                .is_some_and(|hash| hash.starts_with(&format!("{algorithm}:")))
        );

        let drift_directory = tempfile::tempdir()?;
        let drift_candidate = drift_directory.path().join("bench_i001_candidate.csv");
        fs::copy(&candidate, &drift_candidate)?;
        let mut changed = fs::OpenOptions::new().append(true).open(&drift_candidate)?;
        changed.write_all(b"\n")?;
        let drift =
            verify_candidate(&drift_candidate, std::slice::from_ref(&lockfile)).unwrap_err();
        assert_eq!(drift.refusal_code(), "E_INPUT_DRIFT");
        assert!(matches!(drift, LockCheckError::InputDrift { .. }));
    }
    Ok(())
}
