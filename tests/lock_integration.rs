use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

use benchmark::lock_check::{LockCheckError, verify_candidate};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn real_lock_fixture(algorithm: &str) -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
    let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "benchmark-real-lock-{}-{sequence}-{algorithm}",
        std::process::id()
    ));
    fs::create_dir_all(&directory)?;

    let candidate = directory.join("bench_i001_candidate.csv");
    fs::copy(
        fixture("tests/fixtures/candidates/smoke/bench_i001_candidate.csv"),
        &candidate,
    )?;
    let scan = Command::new("vacuum")
        .arg(&directory)
        .arg("--no-witness")
        .output()?;
    if !scan.status.success() {
        return Err(format!(
            "installed vacuum failed: {}",
            String::from_utf8_lossy(&scan.stderr)
        )
        .into());
    }

    let mut hash = Command::new("hashbytes")
        .args(["--algorithm", algorithm, "--no-witness"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    hash.stdin
        .take()
        .ok_or("hashbytes stdin unavailable")?
        .write_all(&scan.stdout)?;
    let hashed = hash.wait_with_output()?;
    if !hashed.status.success() {
        return Err(format!(
            "installed hashbytes failed: {}",
            String::from_utf8_lossy(&hashed.stderr)
        )
        .into());
    }

    let mut lock = Command::new("lock")
        .args(["--dataset-id", "benchmark-real-lock", "--no-witness"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    lock.stdin
        .take()
        .ok_or("lock stdin unavailable")?
        .write_all(&hashed.stdout)?;
    let locked = lock.wait_with_output()?;
    if !locked.status.success() {
        return Err(format!(
            "installed lock failed: {}",
            String::from_utf8_lossy(&locked.stderr)
        )
        .into());
    }

    let lockfile = directory.join(format!("{algorithm}.lock.json"));
    fs::write(&lockfile, locked.stdout)?;
    Ok((candidate, lockfile))
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
fn benchmark_lock_verifies_real_installed_lockfiles_for_sha256_and_blake3()
-> Result<(), Box<dyn std::error::Error>> {
    for algorithm in ["sha256", "blake3"] {
        let (candidate, lockfile) = real_lock_fixture(algorithm)?;
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

        let mut changed = fs::OpenOptions::new().append(true).open(&candidate)?;
        changed.write_all(b"\n")?;
        let drift = verify_candidate(&candidate, std::slice::from_ref(&lockfile)).unwrap_err();
        assert_eq!(drift.refusal_code(), "E_INPUT_DRIFT");
        assert!(matches!(drift, LockCheckError::InputDrift { .. }));
    }
    Ok(())
}
