use std::{
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
};

use duckdb::Connection;
use thiserror::Error;

const RELATION_NAME: &str = "benchmark_candidate";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateFormat {
    Csv,
    Json,
    Jsonl,
    Parquet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSource {
    pub path: PathBuf,
    pub format: CandidateFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateColumn {
    pub name: String,
    pub data_type: String,
}

pub struct LoadedCandidate {
    source: CandidateSource,
    relation_name: String,
    connection: Connection,
    columns: Vec<CandidateColumn>,
}

impl std::fmt::Debug for LoadedCandidate {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LoadedCandidate")
            .field("source", &self.source)
            .field("relation_name", &self.relation_name)
            .field("columns", &self.columns)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum CandidateError {
    #[error("candidate file is unreadable: {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("unsupported candidate format for {path}")]
    FormatDetect { path: PathBuf },
    #[error("candidate is not one row-oriented relation: {detail}")]
    CandidateShape { path: PathBuf, detail: String },
    #[error("failed to load candidate into DuckDB: {path}")]
    Load {
        path: PathBuf,
        #[source]
        source: duckdb::Error,
    },
}

impl CandidateFormat {
    pub fn detect(path: &Path) -> Result<Self, CandidateError> {
        let path_buf = path.to_path_buf();
        let extension = path
            .extension()
            .and_then(OsStr::to_str)
            .map(|value| value.to_ascii_lowercase());

        match extension.as_deref() {
            Some("csv") => Ok(Self::Csv),
            Some("json") => Ok(Self::Json),
            Some("jsonl") => Ok(Self::Jsonl),
            Some("parquet") => Ok(Self::Parquet),
            _ => Err(CandidateError::FormatDetect { path: path_buf }),
        }
    }

    fn load_sql(self, escaped_path: &str) -> String {
        match self {
            Self::Csv => format!(
                "CREATE TEMP TABLE {RELATION_NAME} AS SELECT * FROM read_csv_auto('{escaped_path}');"
            ),
            Self::Json => format!(
                "CREATE TEMP TABLE {RELATION_NAME} AS SELECT * FROM read_json_auto('{escaped_path}');"
            ),
            Self::Jsonl => format!(
                "CREATE TEMP TABLE {RELATION_NAME} AS SELECT * FROM read_ndjson_auto('{escaped_path}');"
            ),
            Self::Parquet => format!(
                "CREATE TEMP TABLE {RELATION_NAME} AS SELECT * FROM read_parquet('{escaped_path}');"
            ),
        }
    }
}

impl CandidateSource {
    pub fn detect(path: impl Into<PathBuf>) -> Result<Self, CandidateError> {
        let path = path.into();
        ensure_candidate_is_file(&path)?;
        let format = CandidateFormat::detect(&path)?;
        Ok(Self { path, format })
    }
}

impl LoadedCandidate {
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, CandidateError> {
        let source = CandidateSource::detect(path)?;
        Self::from_source(source)
    }

    pub fn from_source(source: CandidateSource) -> Result<Self, CandidateError> {
        let connection =
            Connection::open_in_memory().map_err(|source_error| CandidateError::Load {
                path: source.path.clone(),
                source: source_error,
            })?;
        let escaped_path = sql_string_literal(&source.path);
        let load_sql = source.format.load_sql(&escaped_path);
        connection
            .execute_batch(&load_sql)
            .map_err(|source_error| CandidateError::Load {
                path: source.path.clone(),
                source: source_error,
            })?;

        let columns = describe_relation(&connection, RELATION_NAME).map_err(|source_error| {
            CandidateError::Load {
                path: source.path.clone(),
                source: source_error,
            }
        })?;

        if matches!(
            source.format,
            CandidateFormat::Json | CandidateFormat::Jsonl
        ) {
            enforce_row_oriented_json(&source.path, &columns)?;
        }

        Ok(Self {
            source,
            relation_name: RELATION_NAME.to_owned(),
            connection,
            columns,
        })
    }

    pub fn source(&self) -> &CandidateSource {
        &self.source
    }

    pub fn relation_name(&self) -> &str {
        &self.relation_name
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn columns(&self) -> &[CandidateColumn] {
        &self.columns
    }
}

fn ensure_candidate_is_file(path: &Path) -> Result<(), CandidateError> {
    let metadata = fs::metadata(path).map_err(|source| CandidateError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    if metadata.is_file() {
        return Ok(());
    }

    Err(CandidateError::Io {
        path: path.to_path_buf(),
        source: io::Error::other("candidate path is not a file"),
    })
}

fn describe_relation(
    connection: &Connection,
    relation_name: &str,
) -> duckdb::Result<Vec<CandidateColumn>> {
    let mut statement = connection.prepare(&format!("DESCRIBE SELECT * FROM {relation_name}"))?;
    let rows = statement.query_map([], |row| {
        Ok(CandidateColumn {
            name: row.get(0)?,
            data_type: row.get(1)?,
        })
    })?;

    let mut columns = Vec::new();
    for row in rows {
        columns.push(row?);
    }
    Ok(columns)
}

fn enforce_row_oriented_json(
    path: &Path,
    columns: &[CandidateColumn],
) -> Result<(), CandidateError> {
    let nested_columns = columns
        .iter()
        .filter(|column| is_nested_type(&column.data_type))
        .map(|column| format!("{} ({})", column.name, column.data_type))
        .collect::<Vec<_>>();

    if nested_columns.is_empty() {
        return Ok(());
    }

    Err(CandidateError::CandidateShape {
        path: path.to_path_buf(),
        detail: format!(
            "JSON candidate must expose only scalar columns; nested columns: {}",
            nested_columns.join(", ")
        ),
    })
}

fn is_nested_type(data_type: &str) -> bool {
    let normalized = data_type.to_ascii_uppercase();
    normalized == "JSON"
        || normalized.contains("STRUCT(")
        || normalized.contains("MAP(")
        || normalized.contains("UNION(")
        || normalized.contains("LIST")
        || normalized.contains("[]")
}

fn sql_string_literal(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        fs, io,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use duckdb::Connection;

    use super::{
        CandidateError, CandidateFormat, CandidateSource, LoadedCandidate, sql_string_literal,
    };

    static NEXT_TEST_DIR_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> io::Result<Self> {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(io::Error::other)?
                .as_nanos();

            for _ in 0..16 {
                let unique = NEXT_TEST_DIR_ID.fetch_add(1, Ordering::Relaxed);
                let path =
                    std::env::temp_dir().join(format!("benchmark-candidate-{timestamp}-{unique}"));

                match fs::create_dir(&path) {
                    Ok(()) => return Ok(Self { path }),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error),
                }
            }

            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "failed to allocate unique candidate test directory",
            ))
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_fixture(dir: &TestDir, name: &str, contents: &str) -> io::Result<PathBuf> {
        let path = dir.path().join(name);
        fs::write(&path, contents)?;
        Ok(path)
    }

    fn write_parquet_fixture(dir: &TestDir, name: &str) -> Result<PathBuf, Box<dyn Error>> {
        let path = dir.path().join(name);
        let connection = Connection::open_in_memory()?;
        let escaped_path = sql_string_literal(&path);
        let sql = format!(
            "COPY (
                SELECT 'comp_1' AS comp_id, 'Marquis at Briarcliff' AS property_name
                UNION ALL
                SELECT 'comp_2' AS comp_id, 'Briarcliff Commons' AS property_name
            ) TO '{escaped_path}' (FORMAT parquet);"
        );
        connection.execute_batch(&sql)?;
        Ok(path)
    }

    fn count_rows(candidate: &LoadedCandidate) -> duckdb::Result<i64> {
        candidate.connection().query_row(
            &format!("SELECT COUNT(*) FROM {}", candidate.relation_name()),
            [],
            |row| row.get(0),
        )
    }

    #[test]
    fn detects_supported_candidate_formats() -> Result<(), Box<dyn Error>> {
        assert_eq!(
            CandidateFormat::detect(Path::new("candidate.csv"))?,
            CandidateFormat::Csv
        );
        assert_eq!(
            CandidateFormat::detect(Path::new("candidate.json"))?,
            CandidateFormat::Json
        );
        assert_eq!(
            CandidateFormat::detect(Path::new("candidate.jsonl"))?,
            CandidateFormat::Jsonl
        );
        assert_eq!(
            CandidateFormat::detect(Path::new("candidate.parquet"))?,
            CandidateFormat::Parquet
        );
        Ok(())
    }

    #[test]
    fn rejects_unsupported_candidate_extension() -> Result<(), Box<dyn Error>> {
        let error = match CandidateFormat::detect(Path::new("candidate.txt")) {
            Ok(format) => {
                return Err(format!("unexpectedly detected candidate format: {format:?}").into());
            }
            Err(error) => error,
        };
        assert!(matches!(error, CandidateError::FormatDetect { .. }));
        Ok(())
    }

    #[test]
    fn loads_csv_candidate_into_a_temp_relation() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let path = write_fixture(
            &dir,
            "candidate.csv",
            "comp_id,property_name\ncomp_1,Marquis at Briarcliff\ncomp_2,Briarcliff Commons\n",
        )?;

        let candidate = LoadedCandidate::load(path)?;

        assert_eq!(candidate.relation_name(), "benchmark_candidate");
        assert_eq!(count_rows(&candidate)?, 2);
        assert_eq!(candidate.columns().len(), 2);
        Ok(())
    }

    #[test]
    fn loads_jsonl_candidate_into_a_temp_relation() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let path = write_fixture(
            &dir,
            "candidate.jsonl",
            "{\"comp_id\":\"comp_1\",\"property_name\":\"Marquis at Briarcliff\"}\n{\"comp_id\":\"comp_2\",\"property_name\":\"Briarcliff Commons\"}\n",
        )?;

        let candidate = LoadedCandidate::load(path)?;

        assert_eq!(count_rows(&candidate)?, 2);
        assert_eq!(candidate.columns().len(), 2);
        Ok(())
    }

    #[test]
    fn loads_json_array_of_objects_into_a_temp_relation() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let path = write_fixture(
            &dir,
            "candidate.json",
            r#"[{"comp_id":"comp_1","property_name":"Marquis at Briarcliff"},{"comp_id":"comp_2","property_name":"Briarcliff Commons"}]"#,
        )?;

        let candidate = LoadedCandidate::load(path)?;

        assert_eq!(count_rows(&candidate)?, 2);
        assert_eq!(candidate.columns().len(), 2);
        Ok(())
    }

    #[test]
    fn loads_single_object_json_into_a_temp_relation() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let path = write_fixture(
            &dir,
            "candidate.json",
            r#"{"comp_id":"comp_1","property_name":"Marquis at Briarcliff"}"#,
        )?;

        let candidate = LoadedCandidate::load(path)?;

        assert_eq!(count_rows(&candidate)?, 1);
        assert_eq!(candidate.columns().len(), 2);
        Ok(())
    }

    #[test]
    fn loads_parquet_candidate_into_a_temp_relation() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let path = write_parquet_fixture(&dir, "candidate.parquet")?;

        let candidate = LoadedCandidate::load(path)?;

        assert_eq!(count_rows(&candidate)?, 2);
        assert_eq!(candidate.columns().len(), 2);
        Ok(())
    }

    #[test]
    fn rejects_nested_json_candidate_shapes() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let path = write_fixture(
            &dir,
            "candidate.json",
            r#"[{"comp_id":"comp_1","details":{"city":"Atlanta"}}]"#,
        )?;

        let error = match LoadedCandidate::load(path) {
            Ok(candidate) => {
                return Err(
                    format!("unexpectedly loaded nested JSON candidate: {candidate:?}").into(),
                );
            }
            Err(error) => error,
        };

        assert!(matches!(error, CandidateError::CandidateShape { .. }));
        Ok(())
    }

    #[test]
    fn candidate_source_detection_requires_a_real_file() -> Result<(), Box<dyn Error>> {
        let dir = TestDir::new()?;
        let missing = dir.path().join("missing.csv");

        let error = match CandidateSource::detect(missing) {
            Ok(source) => {
                return Err(format!("unexpectedly detected candidate source: {source:?}").into());
            }
            Err(error) => error,
        };

        assert!(matches!(error, CandidateError::Io { .. }));
        Ok(())
    }
}
