use std::path::{Path, PathBuf};

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
