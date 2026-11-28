use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
};

use benchmark::lock_check::{LockCheckError, verify_candidate};
use serde_json::json;
use sha2::{Digest, Sha256};

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(path)
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn write_lock(path: &Path, members: &[(&str, &str)]) -> Result<(), Box<dyn std::error::Error>> {
    let members = members
        .iter()
        .map(|(member_path, bytes_hash)| json!({"path": member_path, "bytes_hash": bytes_hash}))
        .collect::<Vec<_>>();
    fs::write(
        path,
        serde_json::to_vec(&json!({"version": "lock.v0", "members": members}))?,
    )?;
    Ok(())
}

fn with_current_dir<T>(dir: &Path, action: impl FnOnce() -> T) -> T {
    static CURRENT_DIR_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = CURRENT_DIR_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("current-directory test lock");
    let previous = env::current_dir().expect("current directory");
    env::set_current_dir(dir).expect("test directory");
    let result = action();
    env::set_current_dir(previous).expect("restore current directory");
    result
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

#[test]
fn lock_receipt_preserves_the_member_spelling_from_the_lockfile()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let candidate = root.path().join("loans.csv");
    let lockfile = root.path().join("loans.lock.json");
    let bytes = b"comp_id,value\ncomp_1,42\n";
    fs::write(&candidate, bytes)?;
    write_lock(&lockfile, &[("./loans.csv", &sha256_bytes(bytes))])?;

    let verification = verify_candidate(&candidate, std::slice::from_ref(&lockfile))?;
    assert_eq!(verification.verified_member, "./loans.csv");
    Ok(())
}

#[test]
fn lock_drift_detail_preserves_the_member_spelling_from_the_lockfile()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let candidate = root.path().join("loans.csv");
    let lockfile = root.path().join("loans.lock.json");
    let bytes = b"comp_id,value\ncomp_1,42\n";
    fs::write(&candidate, bytes)?;
    write_lock(
        &lockfile,
        &[("./loans.csv", &sha256_bytes(b"different bytes"))],
    )?;

    let error = verify_candidate(&candidate, std::slice::from_ref(&lockfile)).unwrap_err();
    let LockCheckError::InputDrift { member, .. } = error else {
        return Err("expected E_INPUT_DRIFT".into());
    };
    assert_eq!(member, "./loans.csv");
    Ok(())
}

#[test]
fn lock_receipt_prefers_the_legacy_exact_member_spelling_in_one_lock()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let candidate = root.path().join("loans.csv");
    let lockfile = root.path().join("loans.lock.json");
    let bytes = b"comp_id,value\ncomp_1,42\n";
    fs::write(&candidate, bytes)?;
    let actual = sha256_bytes(bytes);
    write_lock(
        &lockfile,
        &[("loans.csv", &actual), ("./loans.csv", &actual)],
    )?;

    let verification = with_current_dir(root.path(), || {
        verify_candidate(Path::new("./loans.csv"), std::slice::from_ref(&lockfile))
    })?;
    assert_eq!(verification.verified_member, "./loans.csv");
    assert_eq!(verification.matched_lockfile, lockfile);
    Ok(())
}

#[test]
fn lock_receipt_prefers_the_legacy_exact_member_spelling_in_two_locks()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let candidate = root.path().join("loans.csv");
    let lockfile_a = root.path().join("first.lock.json");
    let lockfile_b = root.path().join("second.lock.json");
    let bytes = b"comp_id,value\ncomp_1,42\n";
    fs::write(&candidate, bytes)?;
    let actual = sha256_bytes(bytes);
    for lockfile in [&lockfile_a, &lockfile_b] {
        write_lock(
            lockfile,
            &[("loans.csv", &actual), ("./loans.csv", &actual)],
        )?;
    }

    let verification = with_current_dir(root.path(), || {
        verify_candidate(
            Path::new("./loans.csv"),
            &[lockfile_a.clone(), lockfile_b.clone()],
        )
    })?;
    assert_eq!(verification.verified_member, "./loans.csv");
    assert_eq!(verification.matched_lockfile, lockfile_a);
    Ok(())
}

#[test]
fn lock_drift_prefers_the_legacy_exact_member_spelling_in_one_lock()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let candidate = root.path().join("loans.csv");
    let lockfile = root.path().join("loans.lock.json");
    let bytes = b"comp_id,value\ncomp_1,42\n";
    fs::write(&candidate, bytes)?;
    let first_wrong = sha256_bytes(b"first wrong bytes");
    let exact_wrong = sha256_bytes(b"exact wrong bytes");
    write_lock(
        &lockfile,
        &[("loans.csv", &first_wrong), ("./loans.csv", &exact_wrong)],
    )?;

    let error = with_current_dir(root.path(), || {
        verify_candidate(Path::new("./loans.csv"), std::slice::from_ref(&lockfile))
    })
    .unwrap_err();
    let LockCheckError::InputDrift {
        lockfile: drift_lockfile,
        member,
        expected_hash,
        ..
    } = error
    else {
        return Err("expected E_INPUT_DRIFT".into());
    };
    assert_eq!(drift_lockfile, lockfile);
    assert_eq!(member, "./loans.csv");
    assert_eq!(expected_hash, exact_wrong);
    Ok(())
}

#[test]
fn lock_drift_prefers_the_legacy_exact_member_spelling_in_two_locks()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let candidate = root.path().join("loans.csv");
    let lockfile_a = root.path().join("first.lock.json");
    let lockfile_b = root.path().join("second.lock.json");
    let bytes = b"comp_id,value\ncomp_1,42\n";
    fs::write(&candidate, bytes)?;
    let first_wrong = sha256_bytes(b"first wrong bytes");
    let exact_wrong = sha256_bytes(b"exact wrong bytes");
    for lockfile in [&lockfile_a, &lockfile_b] {
        write_lock(
            lockfile,
            &[("loans.csv", &first_wrong), ("./loans.csv", &exact_wrong)],
        )?;
    }

    let error = with_current_dir(root.path(), || {
        verify_candidate(
            Path::new("./loans.csv"),
            &[lockfile_a.clone(), lockfile_b.clone()],
        )
    })
    .unwrap_err();
    let LockCheckError::InputDrift {
        lockfile: drift_lockfile,
        member,
        expected_hash,
        ..
    } = error
    else {
        return Err("expected E_INPUT_DRIFT".into());
    };
    assert_eq!(drift_lockfile, lockfile_a);
    assert_eq!(member, "./loans.csv");
    assert_eq!(expected_hash, exact_wrong);
    Ok(())
}

#[test]
fn conflicting_same_path_lock_digests_refuse_instead_of_accepting_either()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let candidate = root.path().join("candidate.csv");
    let lock_matching = root.path().join("matching.lock.json");
    let lock_different = root.path().join("different.lock.json");
    let bytes = b"comp_id,value\ncomp_1,42\n";
    fs::write(&candidate, bytes)?;
    let actual = sha256_bytes(bytes);
    let different = sha256_bytes(b"a different candidate");
    let candidate_name = candidate.to_str().expect("candidate path");
    write_lock(&lock_matching, &[(candidate_name, &actual)])?;
    write_lock(&lock_different, &[(candidate_name, &different)])?;

    let error = verify_candidate(&candidate, &[lock_matching, lock_different.clone()]).unwrap_err();
    assert_eq!(error.refusal_code(), "E_INPUT_DRIFT");
    let LockCheckError::InputDrift {
        lockfile,
        expected_hash,
        actual_hash,
        ..
    } = error
    else {
        return Err("expected first differing lock member".into());
    };
    {
        assert_eq!(lockfile, lock_different);
        assert_eq!(expected_hash, different);
        assert_eq!(actual_hash, actual);
    }
    Ok(())
}

#[test]
fn parent_folder_two_period_locking_accepts_right_inputs_and_refuses_swapped_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("dec"))?;
    fs::create_dir(root.path().join("nov"))?;
    let dec = b"comp_id,value\ncomp_1,42\n";
    let nov = b"comp_id,value\ncomp_1,43\n";
    fs::write(root.path().join("dec/loans.csv"), dec)?;
    fs::write(root.path().join("nov/loans.csv"), nov)?;
    let assertions = root.path().join("assertions.jsonl");
    fs::write(
        &assertions,
        r#"{"entity":"comp_1","field":"value","expected":"42"}
"#,
    )?;
    let lockfile = root.path().join("parent.lock.json");
    write_lock(
        &lockfile,
        &[
            ("dec/loans.csv", &sha256_bytes(dec)),
            ("nov/loans.csv", &sha256_bytes(nov)),
        ],
    )?;

    let run = |input: &str| -> Result<std::process::Output, std::io::Error> {
        Command::new(env!("CARGO_BIN_EXE_benchmark"))
            .current_dir(root.path())
            .args([
                input,
                "--assertions",
                assertions.to_str().expect("assertions path"),
                "--key",
                "comp_id",
                "--lock",
                lockfile.to_str().expect("lock path"),
                "--json",
                "--no-witness",
            ])
            .output()
    };

    let dec_run = run("./dec/loans.csv")?;
    assert_eq!(dec_run.status.code(), Some(0));
    let dec_report: serde_json::Value = serde_json::from_slice(&dec_run.stdout)?;
    assert_eq!(
        dec_report["input_verification"]["verified_member"],
        "dec/loans.csv"
    );

    let nov_run = run("./nov/loans.csv")?;
    assert_eq!(nov_run.status.code(), Some(1));
    let nov_report: serde_json::Value = serde_json::from_slice(&nov_run.stdout)?;
    assert_eq!(
        nov_report["input_verification"]["verified_member"],
        "nov/loans.csv"
    );

    fs::write(root.path().join("dec/loans.csv"), nov)?;
    let swapped_run = run("./dec/loans.csv")?;
    assert_eq!(swapped_run.status.code(), Some(2));
    let swapped_output = format!(
        "{}{}",
        String::from_utf8_lossy(&swapped_run.stdout),
        String::from_utf8_lossy(&swapped_run.stderr)
    );
    assert!(swapped_output.contains("E_INPUT_DRIFT"), "{swapped_output}");
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_typed_backslash_is_not_reinterpreted_as_a_path_separator()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let candidate = root.path().join(r"candidate\name.csv");
    let normalized_member = root.path().join("candidate/name.csv");
    let lockfile = root.path().join("backslash.lock.json");
    let bytes = b"comp_id,value\ncomp_1,42\n";
    fs::write(&candidate, bytes)?;
    write_lock(
        &lockfile,
        &[(
            normalized_member.to_str().expect("normalized member path"),
            &sha256_bytes(bytes),
        )],
    )?;

    let error = verify_candidate(&candidate, std::slice::from_ref(&lockfile)).unwrap_err();
    assert_eq!(error.refusal_code(), "E_INPUT_NOT_LOCKED");
    assert!(matches!(error, LockCheckError::InputNotLocked { .. }));
    Ok(())
}
