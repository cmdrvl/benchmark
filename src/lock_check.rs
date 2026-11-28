use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use cmdrvl_core::{
    hash::Algorithm,
    lock_matching::{Member, SelectError, Verdict, select},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const E_IO: &str = "E_IO";
pub const E_INPUT_NOT_LOCKED: &str = "E_INPUT_NOT_LOCKED";
pub const E_INPUT_DRIFT: &str = "E_INPUT_DRIFT";
pub const E_UNSUPPORTED_HASH_ALGORITHM: &str = "E_UNSUPPORTED_HASH_ALGORITHM";

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
    let input_name = typed_name_for_matching(&candidate_path.to_string_lossy());
    let mut candidates = Vec::new();

    for (lock, lockfile_path) in lockfiles.iter().enumerate() {
        let lockfile = load_lockfile(lockfile_path)?;
        for member in lockfile.members {
            candidates.push(CandidateMember {
                lock,
                member_path: member.path,
                bytes_hash: member.bytes_hash,
            });
        }
    }

    let members = candidates.iter().map(|candidate| Member {
        lock: candidate.lock,
        path: candidate.member_path.as_str(),
        digest: candidate.bytes_hash.as_str(),
    });
    let selection = match select(&input_name, members) {
        Ok(selection) => selection,
        Err(SelectError::NotListed) => {
            return Err(LockCheckError::InputNotLocked {
                candidate: candidate_path.to_path_buf(),
                lockfiles: lockfiles.to_vec(),
            });
        }
        Err(SelectError::SeveralPaths(members)) => {
            let matches = members
                .iter()
                .map(|member| {
                    let lockfile = lockfiles.get(member.lock).map_or_else(
                        || candidate_path.display().to_string(),
                        |path| path.display().to_string(),
                    );
                    format!("{lockfile}:{}", original_member_path(&candidates, member))
                })
                .collect();
            return Err(LockCheckError::AmbiguousMember {
                candidate: candidate_path.to_path_buf(),
                matches,
            });
        }
        Err(SelectError::UnusableDigest(member)) => {
            return Err(LockCheckError::UnsupportedHashAlgorithm {
                candidate: candidate_path.to_path_buf(),
                lockfile: lockfiles
                    .get(member.lock)
                    .cloned()
                    .unwrap_or_else(|| candidate_path.to_path_buf()),
                member: original_member_path(&candidates, &member),
                algorithm: unsupported_algorithm_name(member.digest),
            });
        }
    };

    let observed: Vec<(Algorithm, String)> = selection
        .algorithms()
        .into_iter()
        .map(|algorithm| {
            let hash_algorithm = match algorithm {
                Algorithm::Sha256 => HashAlgorithm::Sha256,
                Algorithm::Blake3 => HashAlgorithm::Blake3,
            };
            hash_file(candidate_path, hash_algorithm).map(|digest| (algorithm, digest))
        })
        .collect::<Result<_, _>>()?;
    let observed_refs: Vec<(Algorithm, &str)> = observed
        .iter()
        .map(|(algorithm, digest)| (*algorithm, digest.as_str()))
        .collect();

    match selection.judge(&observed_refs) {
        Ok(Verdict::Verified { matched, .. }) => {
            let member = matched.first().ok_or_else(|| LockCheckError::Io {
                path: candidate_path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "R1 selected no matching lock member",
                ),
            })?;
            let member_path = original_member_path(&candidates, member);
            let algorithm = Algorithm::of(member.digest).ok_or_else(|| {
                LockCheckError::UnsupportedHashAlgorithm {
                    candidate: candidate_path.to_path_buf(),
                    lockfile: lockfiles
                        .get(member.lock)
                        .cloned()
                        .unwrap_or_else(|| candidate_path.to_path_buf()),
                    member: member_path.clone(),
                    algorithm: unsupported_algorithm_name(member.digest),
                }
            })?;
            let candidate_hash = observed
                .iter()
                .find_map(|(kind, digest)| match (*kind, algorithm) {
                    (Algorithm::Sha256, Algorithm::Sha256)
                    | (Algorithm::Blake3, Algorithm::Blake3) => Some(digest.clone()),
                    _ => None,
                })
                .ok_or_else(|| LockCheckError::Io {
                    path: candidate_path.to_path_buf(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "R1 did not compute the selected lock digest",
                    ),
                })?;
            Ok(InputVerification {
                lockfiles: lockfiles.to_vec(),
                matched_lockfile: lockfiles
                    .get(member.lock)
                    .cloned()
                    .unwrap_or_else(|| candidate_path.to_path_buf()),
                verified_member: member_path,
                candidate_hash,
            })
        }
        Ok(Verdict::Drift { differing, .. }) => {
            let member = differing.first().ok_or_else(|| LockCheckError::Io {
                path: candidate_path.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "R1 reported drift without a differing lock member",
                ),
            })?;
            let member_path = original_member_path(&candidates, member);
            let algorithm = Algorithm::of(member.digest).ok_or_else(|| {
                LockCheckError::UnsupportedHashAlgorithm {
                    candidate: candidate_path.to_path_buf(),
                    lockfile: lockfiles
                        .get(member.lock)
                        .cloned()
                        .unwrap_or_else(|| candidate_path.to_path_buf()),
                    member: member_path.clone(),
                    algorithm: unsupported_algorithm_name(member.digest),
                }
            })?;
            let actual_hash = observed
                .iter()
                .find_map(|(kind, digest)| match (*kind, algorithm) {
                    (Algorithm::Sha256, Algorithm::Sha256)
                    | (Algorithm::Blake3, Algorithm::Blake3) => Some(digest.clone()),
                    _ => None,
                })
                .ok_or_else(|| LockCheckError::Io {
                    path: candidate_path.to_path_buf(),
                    source: std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "R1 did not compute the differing lock digest",
                    ),
                })?;
            Err(LockCheckError::InputDrift {
                candidate: candidate_path.to_path_buf(),
                lockfile: lockfiles
                    .get(member.lock)
                    .cloned()
                    .unwrap_or_else(|| candidate_path.to_path_buf()),
                member: member_path,
                expected_hash: member.digest.to_owned(),
                actual_hash,
            })
        }
        Err(missing) => Err(LockCheckError::Io {
            path: candidate_path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "R1 did not compute the {} lock digest",
                    missing.algorithm.prefix()
                ),
            ),
        }),
    }
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
    #[error(
        "unsupported bytes_hash algorithm '{algorithm}' in lockfile '{lockfile}' member '{member}'"
    )]
    UnsupportedHashAlgorithm {
        candidate: PathBuf,
        lockfile: PathBuf,
        member: String,
        algorithm: String,
    },
}

impl LockCheckError {
    pub const fn refusal_code(&self) -> &'static str {
        match self {
            Self::Io { .. } | Self::Parse { .. } => E_IO,
            Self::InputNotLocked { .. } | Self::AmbiguousMember { .. } => E_INPUT_NOT_LOCKED,
            Self::InputDrift { .. } => E_INPUT_DRIFT,
            Self::UnsupportedHashAlgorithm { .. } => E_UNSUPPORTED_HASH_ALGORITHM,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CandidateMember {
    lock: usize,
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

    match lockfile.version.as_str() {
        "lock.v0" => {}
        _ => {
            return Err(LockCheckError::Parse {
                path: path.to_path_buf(),
                source: serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unsupported lockfile version '{}'", lockfile.version),
                )),
            });
        }
    }

    Ok(lockfile)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HashAlgorithm {
    Sha256,
    Blake3,
}

fn hash_file(path: &Path, algorithm: HashAlgorithm) -> Result<String, LockCheckError> {
    let mut file = File::open(path).map_err(|source| LockCheckError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut buffer = [0_u8; 8192];

    match algorithm {
        HashAlgorithm::Sha256 => {
            let mut hasher = Sha256::new();
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
        HashAlgorithm::Blake3 => {
            let mut hasher = blake3::Hasher::new();
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
            Ok(format!("blake3:{}", hasher.finalize().to_hex()))
        }
    }
}

fn unsupported_algorithm_name(bytes_hash: &str) -> String {
    bytes_hash
        .split_once(':')
        .map_or(bytes_hash, |(algorithm, _)| algorithm)
        .to_owned()
}

fn original_member_path(candidates: &[CandidateMember], member: &Member<'_>) -> String {
    candidates
        .iter()
        .find(|candidate| {
            candidate.lock.eq(&member.lock)
                && normalize_lock_member_path(&candidate.member_path).eq(member.path)
                && candidate.bytes_hash.eq(member.digest)
        })
        .map_or_else(
            || member.path.to_owned(),
            |candidate| candidate.member_path.clone(),
        )
}

fn normalize_lock_member_path(path: &str) -> &str {
    let mut normalized = path;
    while let Some(stripped) = normalized.strip_prefix("./") {
        normalized = stripped;
    }
    normalized
}

#[cfg(windows)]
fn typed_name_for_matching(file: &str) -> String {
    file.replace('\\', "/")
}

#[cfg(not(windows))]
fn typed_name_for_matching(file: &str) -> String {
    file.to_owned()
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::{
        E_INPUT_DRIFT, E_INPUT_NOT_LOCKED, E_UNSUPPORTED_HASH_ALGORITHM, HashAlgorithm,
        InputVerification, LockCheckError, verify_candidate,
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

        let candidate_hash = super::hash_file(&candidate, HashAlgorithm::Sha256)?;
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
    fn bench_u_lock_refuses_unusable_digest() -> Result<(), Box<dyn std::error::Error>> {
        let candidate = temp_path("unusable-digest.csv");
        let lockfile = temp_path("unusable-digest.lock.json");
        let candidate_name = candidate
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("candidate file name")?;
        write_file(&candidate, "comp_id,value\ncomp_1,42\n")?;
        write_file(
            &lockfile,
            &format!(
                "{{\"version\":\"lock.v0\",\"members\":[{{\"path\":\"{candidate_name}\",\"bytes_hash\":\"sha256:deadbeef\"}}]}}"
            ),
        )?;

        let error = verify_candidate(&candidate, std::slice::from_ref(&lockfile)).unwrap_err();
        assert_eq!(error.refusal_code(), E_UNSUPPORTED_HASH_ALGORITHM);
        assert!(matches!(
            error,
            LockCheckError::UnsupportedHashAlgorithm { .. }
        ));

        remove_if_exists(&candidate)?;
        remove_if_exists(&lockfile)?;
        Ok(())
    }

    #[test]
    fn bench_u_lock_refuses_unknown_hash_algorithm() -> Result<(), Box<dyn std::error::Error>> {
        let candidate = temp_path("unsupported-algorithm.csv");
        let lockfile = temp_path("unsupported-algorithm.lock.json");
        let candidate_name = candidate
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("candidate path is not valid UTF-8")?;
        write_file(&candidate, "comp_id,value\ncomp_1,42\n")?;
        write_file(
            &lockfile,
            &format!(
                "{{\"version\":\"lock.v0\",\"members\":[{{\"path\":\"{candidate_name}\",\"bytes_hash\":\"md5:{hash}\"}}]}}\n",
                hash = "0".repeat(64),
            ),
        )?;

        let error = verify_candidate(&candidate, std::slice::from_ref(&lockfile)).unwrap_err();
        assert_eq!(error.refusal_code(), E_UNSUPPORTED_HASH_ALGORITHM);
        assert!(
            error
                .to_string()
                .contains("unsupported bytes_hash algorithm 'md5'")
        );
        assert!(matches!(
            error,
            LockCheckError::UnsupportedHashAlgorithm { .. }
        ));
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

        for (lockfile, folder) in [(&lockfile_a, "a"), (&lockfile_b, "b")] {
            write_file(
                lockfile,
                &format!(
                    "{{\"version\":\"lock.v0\",\"lock_hash\":\"sha256:{hash}\",\"dataset_id\":null,\"as_of\":null,\"note\":null,\"created\":\"2026-03-08T00:00:00Z\",\"tool_versions\":{{\"lock\":\"0.2.0\"}},\"profiles\":[],\"skipped\":[],\"members\":[{{\"path\":\"nested/{folder}/{candidate_name}\",\"bytes_hash\":\"sha256:{member_hash}\",\"size\":24,\"fingerprint\":null}}],\"skipped_count\":0,\"member_count\":1}}",
                    hash = "0".repeat(64),
                    member_hash = "3".repeat(64),
                ),
            )?;
        }

        let error =
            verify_candidate(&candidate, &[lockfile_a.clone(), lockfile_b.clone()]).unwrap_err();
        assert_eq!(error.refusal_code(), E_INPUT_NOT_LOCKED);
        let LockCheckError::AmbiguousMember { matches, .. } = error else {
            return Err("expected E_INPUT_NOT_LOCKED ambiguity details".into());
        };
        assert_eq!(matches.len(), 2);
        assert!(
            matches
                .iter()
                .any(|detail| detail.ends_with(&format!(":nested/a/{candidate_name}"))),
            "matches: {matches:?}"
        );
        assert!(
            matches
                .iter()
                .any(|detail| detail.ends_with(&format!(":nested/b/{candidate_name}"))),
            "matches: {matches:?}"
        );

        remove_if_exists(&candidate)?;
        remove_if_exists(&lockfile_a)?;
        remove_if_exists(&lockfile_b)?;
        Ok(())
    }
}
