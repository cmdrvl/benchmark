use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const E_IO: &str = "E_IO";
pub const E_INPUT_NOT_LOCKED: &str = "E_INPUT_NOT_LOCKED";
pub const E_INPUT_DRIFT: &str = "E_INPUT_DRIFT";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputVerification {
    pub lockfiles: Vec<PathBuf>,
    pub matched_lockfile: PathBuf,
    pub verified_member: String,
    pub candidate_hash: String,
}

pub fn verify_candidate(
    candidate_path: impl AsRef<Path>,
    lockfiles: &[PathBuf],
) -> Result<InputVerification, LockCheckError> {
    let candidate_path = candidate_path.as_ref();
    let candidate_hash = hash_file(candidate_path)?;
    let exact_candidate_path = normalize_path(candidate_path);
    let candidate_basename = candidate_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned);

    let mut exact_matches = Vec::new();
    let mut basename_matches = Vec::new();

    for lockfile_path in lockfiles {
        let lockfile = load_lockfile(lockfile_path)?;
        for member in lockfile.members {
            let candidate_member = CandidateMember {
                lockfile: lockfile_path.clone(),
                member_path: member.path,
                bytes_hash: member.bytes_hash,
            };

            if candidate_member.member_path == exact_candidate_path {
                exact_matches.push(candidate_member);
                continue;
            }

            if let Some(basename) = &candidate_basename
                && Path::new(&candidate_member.member_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    == Some(basename.as_str())
            {
                basename_matches.push(candidate_member);
            }
        }
    }

    if let Some(verification) =
        verify_matches(candidate_path, lockfiles, &candidate_hash, exact_matches)?
    {
        return Ok(verification);
    }

    let basename_matches = dedupe_matches(basename_matches);
    if basename_matches.len() > 1 {
        return Err(LockCheckError::AmbiguousMember {
            candidate: candidate_path.to_path_buf(),
            matches: basename_matches
                .iter()
                .map(|candidate| {
                    format!("{}:{}", candidate.lockfile.display(), candidate.member_path)
                })
                .collect(),
        });
    }

    if let Some(verification) =
        verify_matches(candidate_path, lockfiles, &candidate_hash, basename_matches)?
    {
        return Ok(verification);
    }

    Err(LockCheckError::InputNotLocked {
        candidate: candidate_path.to_path_buf(),
        lockfiles: lockfiles.to_vec(),
    })
}

#[derive(Debug, Error)]
pub enum LockCheckError {
    #[error("failed to read '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse lockfile '{path}': {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("candidate '{candidate}' is not present in the provided lockfiles")]
    InputNotLocked {
        candidate: PathBuf,
        lockfiles: Vec<PathBuf>,
    },
    #[error("candidate '{candidate}' matched multiple lock members and cannot be verified safely")]
    AmbiguousMember {
        candidate: PathBuf,
        matches: Vec<String>,
    },
    #[error(
        "candidate '{candidate}' hash drifted from lockfile '{lockfile}' member '{member}': expected {expected_hash}, got {actual_hash}"
    )]
    InputDrift {
        candidate: PathBuf,
        lockfile: PathBuf,
        member: String,
        expected_hash: String,
        actual_hash: String,
    },
}

impl LockCheckError {
    pub const fn refusal_code(&self) -> &'static str {
        match self {
            Self::Io { .. } | Self::Parse { .. } => E_IO,
            Self::InputNotLocked { .. } | Self::AmbiguousMember { .. } => E_INPUT_NOT_LOCKED,
            Self::InputDrift { .. } => E_INPUT_DRIFT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CandidateMember {
    lockfile: PathBuf,
    member_path: String,
    bytes_hash: String,
}

#[derive(Debug, Deserialize)]
struct Lockfile {
    version: String,
    members: Vec<LockMember>,
}

#[derive(Debug, Deserialize)]
struct LockMember {
    path: String,
    bytes_hash: String,
}

fn load_lockfile(path: &Path) -> Result<Lockfile, LockCheckError> {
    let contents = std::fs::read_to_string(path).map_err(|source| LockCheckError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let lockfile: Lockfile =
        serde_json::from_str(&contents).map_err(|source| LockCheckError::Parse {
            path: path.to_path_buf(),
            source,
        })?;

    if lockfile.version != "lock.v0" {
        return Err(LockCheckError::Parse {
            path: path.to_path_buf(),
            source: serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unsupported lockfile version '{}'", lockfile.version),
            )),
        });
    }

    Ok(lockfile)
}

fn hash_file(path: &Path) -> Result<String, LockCheckError> {
    let mut file = File::open(path).map_err(|source| LockCheckError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|source| LockCheckError::Io {
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn dedupe_matches(matches: Vec<CandidateMember>) -> Vec<CandidateMember> {
    let mut deduped = Vec::new();

    for candidate in matches {
        if deduped.iter().any(|seen: &CandidateMember| {
            seen.lockfile == candidate.lockfile && seen.member_path == candidate.member_path
        }) {
            continue;
        }
        deduped.push(candidate);
    }

    deduped
}

fn verify_matches(
    candidate_path: &Path,
    lockfiles: &[PathBuf],
    candidate_hash: &str,
    matches: Vec<CandidateMember>,
) -> Result<Option<InputVerification>, LockCheckError> {
    if matches.is_empty() {
        return Ok(None);
    }

    if let Some(candidate) = matches
        .iter()
        .find(|candidate| candidate.bytes_hash == candidate_hash)
    {
        return Ok(Some(InputVerification {
            lockfiles: lockfiles.to_vec(),
            matched_lockfile: candidate.lockfile.clone(),
            verified_member: candidate.member_path.clone(),
            candidate_hash: candidate_hash.to_owned(),
        }));
    }

    let drift = &matches[0];
    Err(LockCheckError::InputDrift {
        candidate: candidate_path.to_path_buf(),
        lockfile: drift.lockfile.clone(),
        member: drift.member_path.clone(),
        expected_hash: drift.bytes_hash.clone(),
        actual_hash: candidate_hash.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{
        E_INPUT_DRIFT, E_INPUT_NOT_LOCKED, InputVerification, LockCheckError, verify_candidate,
    };

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_path(name: &str) -> PathBuf {
        let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!(
            "benchmark-lock-{name}-{}-{unique}",
            std::process::id()
        ))
    }

    fn write_file(path: &PathBuf, contents: &str) -> Result<(), std::io::Error> {
        fs::write(path, contents)
    }

    fn remove_if_exists(path: &PathBuf) -> Result<(), std::io::Error> {
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    #[test]
    fn bench_u_lock_verifies_member_hash() -> Result<(), Box<dyn std::error::Error>> {
        let candidate = temp_path("candidate.csv");
        let lockfile = temp_path("candidate.lock.json");
        write_file(&candidate, "comp_id,value\ncomp_1,42\n")?;

        let verification = verify_candidate(&candidate, &[] as &[PathBuf]);
        assert!(matches!(
            verification,
            Err(LockCheckError::InputNotLocked { .. })
        ));

        let candidate_hash = super::hash_file(&candidate)?;
        let candidate_name = candidate
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("candidate file name")?;
        write_file(
            &lockfile,
            &format!(
                "{{\"version\":\"lock.v0\",\"lock_hash\":\"sha256:{hash}\",\"dataset_id\":null,\"as_of\":null,\"note\":null,\"created\":\"2026-03-08T00:00:00Z\",\"tool_versions\":{{\"lock\":\"0.2.0\"}},\"profiles\":[],\"skipped\":[],\"members\":[{{\"path\":\"{candidate_name}\",\"bytes_hash\":\"{candidate_hash}\",\"size\":24,\"fingerprint\":null}}],\"skipped_count\":0,\"member_count\":1}}",
                hash = "0".repeat(64),
            ),
        )?;

        let verification = verify_candidate(&candidate, std::slice::from_ref(&lockfile))?;
        assert_eq!(
            verification,
            InputVerification {
                lockfiles: vec![lockfile.clone()],
                matched_lockfile: lockfile.clone(),
                verified_member: candidate_name.to_owned(),
                candidate_hash,
            }
        );

        remove_if_exists(&candidate)?;
        remove_if_exists(&lockfile)?;
        Ok(())
    }

    #[test]
    fn bench_u_lock_refuses_non_member() -> Result<(), Box<dyn std::error::Error>> {
        let candidate = temp_path("missing.csv");
        let lockfile = temp_path("missing.lock.json");
        write_file(&candidate, "comp_id,value\ncomp_1,42\n")?;
        write_file(
            &lockfile,
            &format!(
                "{{\"version\":\"lock.v0\",\"lock_hash\":\"sha256:{hash}\",\"dataset_id\":null,\"as_of\":null,\"note\":null,\"created\":\"2026-03-08T00:00:00Z\",\"tool_versions\":{{\"lock\":\"0.2.0\"}},\"profiles\":[],\"skipped\":[],\"members\":[{{\"path\":\"other.csv\",\"bytes_hash\":\"sha256:{member_hash}\",\"size\":10,\"fingerprint\":null}}],\"skipped_count\":0,\"member_count\":1}}",
                hash = "0".repeat(64),
                member_hash = "1".repeat(64),
            ),
        )?;

        let error = verify_candidate(&candidate, std::slice::from_ref(&lockfile)).unwrap_err();
        assert_eq!(error.refusal_code(), E_INPUT_NOT_LOCKED);

        remove_if_exists(&candidate)?;
        remove_if_exists(&lockfile)?;
        Ok(())
    }

    #[test]
    fn bench_u_lock_refuses_hash_drift() -> Result<(), Box<dyn std::error::Error>> {
        let candidate = temp_path("drift.csv");
        let lockfile = temp_path("drift.lock.json");
        write_file(&candidate, "comp_id,value\ncomp_1,42\n")?;
        let candidate_name = candidate
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("candidate file name")?;
        write_file(
            &lockfile,
            &format!(
                "{{\"version\":\"lock.v0\",\"lock_hash\":\"sha256:{hash}\",\"dataset_id\":null,\"as_of\":null,\"note\":null,\"created\":\"2026-03-08T00:00:00Z\",\"tool_versions\":{{\"lock\":\"0.2.0\"}},\"profiles\":[],\"skipped\":[],\"members\":[{{\"path\":\"{candidate_name}\",\"bytes_hash\":\"sha256:{member_hash}\",\"size\":24,\"fingerprint\":null}}],\"skipped_count\":0,\"member_count\":1}}",
                hash = "0".repeat(64),
                member_hash = "2".repeat(64),
            ),
        )?;

        let error = verify_candidate(&candidate, std::slice::from_ref(&lockfile)).unwrap_err();
        assert_eq!(error.refusal_code(), E_INPUT_DRIFT);
        assert!(matches!(error, LockCheckError::InputDrift { .. }));

        remove_if_exists(&candidate)?;
        remove_if_exists(&lockfile)?;
        Ok(())
    }

    #[test]
    fn bench_u_lock_refuses_ambiguous_basename_matches() -> Result<(), Box<dyn std::error::Error>> {
        let candidate = temp_path("duplicate.csv");
        let lockfile_a = temp_path("a.lock.json");
        let lockfile_b = temp_path("b.lock.json");
        write_file(&candidate, "comp_id,value\ncomp_1,42\n")?;
        let candidate_name = candidate
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("candidate file name")?;

        for lockfile in [&lockfile_a, &lockfile_b] {
            write_file(
                lockfile,
                &format!(
                    "{{\"version\":\"lock.v0\",\"lock_hash\":\"sha256:{hash}\",\"dataset_id\":null,\"as_of\":null,\"note\":null,\"created\":\"2026-03-08T00:00:00Z\",\"tool_versions\":{{\"lock\":\"0.2.0\"}},\"profiles\":[],\"skipped\":[],\"members\":[{{\"path\":\"nested/{candidate_name}\",\"bytes_hash\":\"sha256:{member_hash}\",\"size\":24,\"fingerprint\":null}}],\"skipped_count\":0,\"member_count\":1}}",
                    hash = "0".repeat(64),
                    member_hash = "3".repeat(64),
                ),
            )?;
        }

        let error =
            verify_candidate(&candidate, &[lockfile_a.clone(), lockfile_b.clone()]).unwrap_err();
        assert_eq!(error.refusal_code(), E_INPUT_NOT_LOCKED);
        assert!(matches!(error, LockCheckError::AmbiguousMember { .. }));

        remove_if_exists(&candidate)?;
        remove_if_exists(&lockfile_a)?;
        remove_if_exists(&lockfile_b)?;
        Ok(())
    }
}
