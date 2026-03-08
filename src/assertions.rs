use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use crate::compare::CompareAs;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const E_IO: &str = "E_IO";
pub const E_BAD_ASSERTIONS: &str = "E_BAD_ASSERTIONS";
pub const E_EMPTY_ASSERTIONS: &str = "E_EMPTY_ASSERTIONS";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assertion {
    pub entity: String,
    pub field: String,
    pub expected: String,
    #[serde(default)]
    pub compare_as: CompareAs,
    pub tolerance: Option<f64>,
    #[serde(default)]
    pub severity: Severity,
    pub source: Option<String>,
}

impl Assertion {
    fn validate(&self, path: &Path, line: usize) -> Result<(), AssertionError> {
        if let Some(tolerance) = self.tolerance
            && tolerance < 0.0
        {
            return Err(AssertionError::Semantic {
                path: path.to_path_buf(),
                line,
                message: "tolerance must be non-negative".to_owned(),
            });
        }

        if self.tolerance.is_some()
            && !matches!(self.compare_as, CompareAs::Number | CompareAs::Percent)
        {
            return Err(AssertionError::Semantic {
                path: path.to_path_buf(),
                line,
                message: format!(
                    "tolerance is only valid for number and percent assertions, not {}",
                    compare_as_name(self.compare_as)
                ),
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AssertionSet {
    pub assertions: Vec<Assertion>,
}

impl AssertionSet {
    pub fn load_jsonl(path: impl AsRef<Path>) -> Result<Self, AssertionError> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path).map_err(|source| AssertionError::Io {
            path: path.clone(),
            source,
        })?;
        let reader = BufReader::new(file);
        let mut assertions = Vec::new();

        for (index, line_result) in reader.lines().enumerate() {
            let line_number = index + 1;
            let line = line_result.map_err(|source| AssertionError::Io {
                path: path.clone(),
                source,
            })?;

            if line.trim().is_empty() {
                continue;
            }

            let assertion: Assertion =
                serde_json::from_str(&line).map_err(|source| AssertionError::Parse {
                    path: path.clone(),
                    line: line_number,
                    source,
                })?;
            assertion.validate(&path, line_number)?;
            assertions.push(assertion);
        }

        if assertions.is_empty() {
            return Err(AssertionError::Empty { path });
        }

        Ok(Self { assertions })
    }

    pub fn len(&self) -> usize {
        self.assertions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assertions.is_empty()
    }
}

pub fn load_assertions(path: impl AsRef<Path>) -> Result<AssertionSet, AssertionError> {
    AssertionSet::load_jsonl(path)
}

#[derive(Debug, Error)]
pub enum AssertionError {
    #[error("failed to read assertions file '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse assertion JSONL at line {line} in '{path}': {source}")]
    Parse {
        path: PathBuf,
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("invalid assertion at line {line} in '{path}': {message}")]
    Semantic {
        path: PathBuf,
        line: usize,
        message: String,
    },
    #[error("assertions file '{path}' contains zero valid assertions")]
    Empty { path: PathBuf },
}

impl AssertionError {
    pub const fn refusal_code(&self) -> &'static str {
        match self {
            Self::Io { .. } => E_IO,
            Self::Parse { .. } | Self::Semantic { .. } => E_BAD_ASSERTIONS,
            Self::Empty { .. } => E_EMPTY_ASSERTIONS,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    #[default]
    Major,
    Minor,
}

const fn compare_as_name(compare_as: CompareAs) -> &'static str {
    match compare_as {
        CompareAs::String => "string",
        CompareAs::Number => "number",
        CompareAs::Percent => "percent",
        CompareAs::Date => "date",
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use crate::{
        assertions::{AssertionError, AssertionSet, E_BAD_ASSERTIONS, E_EMPTY_ASSERTIONS},
        compare::CompareAs,
    };

    static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn write_temp_assertions(contents: &str) -> std::io::Result<PathBuf> {
        let unique = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "benchmark-assertions-{}-{unique}.jsonl",
            std::process::id()
        ));
        fs::write(&path, contents)?;
        Ok(path)
    }

    #[allow(non_snake_case)]
    #[test]
    fn BENCH_U001_malformed_jsonl_line_refuses_with_bad_assertions()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = write_temp_assertions(
            "{\"entity\":\"comp_1\",\"field\":\"u8:cap_rate\",\"expected\":\"6.76%\"}\nnot-json\n",
        )?;

        let error = match AssertionSet::load_jsonl(&path) {
            Ok(_) => return Err("malformed JSONL should refuse".into()),
            Err(error) => error,
        };
        assert_eq!(error.refusal_code(), E_BAD_ASSERTIONS);
        assert!(matches!(error, AssertionError::Parse { line: 2, .. }));

        fs::remove_file(path)?;
        Ok(())
    }

    #[allow(non_snake_case)]
    #[test]
    fn BENCH_U002_illegal_tolerance_combination_refuses_with_bad_assertions()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = write_temp_assertions(
            "{\"entity\":\"comp_1\",\"field\":\"u8:cap_rate\",\"expected\":\"6.76%\",\"compare_as\":\"string\",\"tolerance\":0.01}\n",
        )?;

        let error = match AssertionSet::load_jsonl(&path) {
            Ok(_) => return Err("tolerance on string comparisons should refuse".into()),
            Err(error) => error,
        };
        assert_eq!(error.refusal_code(), E_BAD_ASSERTIONS);
        assert!(matches!(error, AssertionError::Semantic { line: 1, .. }));

        fs::remove_file(path)?;
        Ok(())
    }

    #[allow(non_snake_case)]
    #[test]
    fn BENCH_U003_loader_defaults_and_preserves_stable_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = write_temp_assertions(
            "{\"entity\":\"comp_1\",\"field\":\"u8:name\",\"expected\":\"Marquis\"}\n\
             {\"entity\":\"comp_2\",\"field\":\"u8:sale_price\",\"expected\":\"28200000\",\"compare_as\":\"number\",\"tolerance\":1000,\"severity\":\"critical\",\"source\":\"reference_excel:D5\"}\n",
        )?;

        let set = AssertionSet::load_jsonl(&path)?;
        assert_eq!(set.len(), 2);

        assert_eq!(set.assertions[0].entity, "comp_1");
        assert_eq!(set.assertions[0].compare_as, CompareAs::String);
        assert_eq!(
            set.assertions[0].severity,
            crate::assertions::Severity::Major
        );
        assert_eq!(set.assertions[0].tolerance, None);

        assert_eq!(set.assertions[1].entity, "comp_2");
        assert_eq!(set.assertions[1].compare_as, CompareAs::Number);
        assert_eq!(set.assertions[1].tolerance, Some(1000.0));
        assert_eq!(
            set.assertions[1].severity,
            crate::assertions::Severity::Critical
        );
        assert_eq!(
            set.assertions[1].source.as_deref(),
            Some("reference_excel:D5")
        );

        fs::remove_file(path)?;
        Ok(())
    }

    #[allow(non_snake_case)]
    #[test]
    fn BENCH_U004_blank_or_empty_file_refuses_with_empty_assertions()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = write_temp_assertions("\n  \n")?;

        let error = match AssertionSet::load_jsonl(&path) {
            Ok(_) => return Err("blank assertions file should refuse".into()),
            Err(error) => error,
        };
        assert_eq!(error.refusal_code(), E_EMPTY_ASSERTIONS);
        assert!(matches!(error, AssertionError::Empty { .. }));

        fs::remove_file(path)?;
        Ok(())
    }

    #[allow(non_snake_case)]
    #[test]
    fn BENCH_U005_negative_tolerance_refuses_with_bad_assertions()
    -> Result<(), Box<dyn std::error::Error>> {
        let path = write_temp_assertions(
            "{\"entity\":\"comp_1\",\"field\":\"u8:cap_rate\",\"expected\":\"6.76%\",\"compare_as\":\"percent\",\"tolerance\":-0.01}\n",
        )?;

        let error = match AssertionSet::load_jsonl(&path) {
            Ok(_) => return Err("negative tolerance should refuse".into()),
            Err(error) => error,
        };
        assert_eq!(error.refusal_code(), E_BAD_ASSERTIONS);
        assert!(matches!(
            error,
            AssertionError::Semantic {
                line: 1,
                message,
                ..
            } if message.contains("non-negative")
        ));

        fs::remove_file(path)?;
        Ok(())
    }
}
