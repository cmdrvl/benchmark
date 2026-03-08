use serde_json::{Value, json};
use thiserror::Error;

use crate::candidate::LoadedCandidate;

pub const E_IO: &str = "E_IO";
pub const E_KEY_NOT_FOUND: &str = "E_KEY_NOT_FOUND";
pub const E_KEY_NOT_UNIQUE: &str = "E_KEY_NOT_UNIQUE";
pub const E_KEY_NULL: &str = "E_KEY_NULL";

const DUPLICATE_SAMPLE_LIMIT: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyCheckResult {
    pub key_column: String,
    pub benchmarked_rows: usize,
}

pub fn validate_key(
    candidate: &LoadedCandidate,
    key_column: impl Into<String>,
) -> Result<KeyCheckResult, KeyCheckError> {
    let key_column = key_column.into();
    ensure_key_exists(candidate, &key_column)?;

    let relation_identifier = sql_identifier(candidate.relation_name());
    let key_identifier = sql_identifier(&key_column);
    let key_value = format!("CAST({key_identifier} AS VARCHAR)");
    let key_trimmed = format!("TRIM({key_value})");

    let sample_entities = duplicate_entities(
        candidate,
        &relation_identifier,
        &key_identifier,
        &key_value,
        &key_trimmed,
    )
    .map_err(|source| KeyCheckError::Query {
        key_column: key_column.clone(),
        source,
    })?;
    if !sample_entities.is_empty() {
        return Err(KeyCheckError::KeyNotUnique {
            key_column,
            sample_entities,
        });
    }

    let null_rows = null_or_blank_row_count(
        candidate,
        &relation_identifier,
        &key_identifier,
        &key_trimmed,
    )
    .map_err(|source| KeyCheckError::Query {
        key_column: key_column.clone(),
        source,
    })?;
    let null_rows = row_count_to_usize(&key_column, null_rows)?;
    if null_rows > 0 {
        return Err(KeyCheckError::KeyNull {
            key_column,
            null_rows,
        });
    }

    let benchmarked_rows =
        benchmarked_row_count(candidate, &relation_identifier).map_err(|source| {
            KeyCheckError::Query {
                key_column: key_column.clone(),
                source,
            }
        })?;
    let benchmarked_rows = row_count_to_usize(&key_column, benchmarked_rows)?;

    Ok(KeyCheckResult {
        key_column,
        benchmarked_rows,
    })
}

#[derive(Debug, Error)]
pub enum KeyCheckError {
    #[error("candidate key column '{key_column}' was not found")]
    KeyNotFound {
        key_column: String,
        available_columns: Vec<String>,
    },
    #[error("candidate key column '{key_column}' contains duplicate values")]
    KeyNotUnique {
        key_column: String,
        sample_entities: Vec<String>,
    },
    #[error("candidate key column '{key_column}' contains null or blank values")]
    KeyNull {
        key_column: String,
        null_rows: usize,
    },
    #[error("failed to validate candidate key column '{key_column}': {source}")]
    Query {
        key_column: String,
        #[source]
        source: duckdb::Error,
    },
    #[error("candidate row count for key column '{key_column}' does not fit in usize: {row_count}")]
    RowCountOverflow { key_column: String, row_count: i64 },
}

impl KeyCheckError {
    pub const fn refusal_code(&self) -> &'static str {
        match self {
            Self::KeyNotFound { .. } => E_KEY_NOT_FOUND,
            Self::KeyNotUnique { .. } => E_KEY_NOT_UNIQUE,
            Self::KeyNull { .. } => E_KEY_NULL,
            Self::Query { .. } | Self::RowCountOverflow { .. } => E_IO,
        }
    }

    pub fn refusal_detail(&self) -> Value {
        match self {
            Self::KeyNotFound {
                key_column,
                available_columns,
            } => json!({
                "key_column": key_column,
                "available_columns": available_columns,
            }),
            Self::KeyNotUnique {
                key_column,
                sample_entities,
            } => json!({
                "key_column": key_column,
                "sample_entities": sample_entities,
            }),
            Self::KeyNull {
                key_column,
                null_rows,
            } => json!({
                "key_column": key_column,
                "null_rows": null_rows,
            }),
            Self::Query { key_column, .. } => json!({
                "key_column": key_column,
            }),
            Self::RowCountOverflow {
                key_column,
                row_count,
            } => json!({
                "key_column": key_column,
                "row_count": row_count,
            }),
        }
    }
}

fn ensure_key_exists(candidate: &LoadedCandidate, key_column: &str) -> Result<(), KeyCheckError> {
    if candidate
        .columns()
        .iter()
        .any(|column| column.name == key_column)
    {
        return Ok(());
    }

    Err(KeyCheckError::KeyNotFound {
        key_column: key_column.to_owned(),
        available_columns: candidate
            .columns()
            .iter()
            .map(|column| column.name.clone())
            .collect(),
    })
}

fn duplicate_entities(
    candidate: &LoadedCandidate,
    relation_identifier: &str,
    key_identifier: &str,
    key_value: &str,
    key_trimmed: &str,
) -> duckdb::Result<Vec<String>> {
    let sql = format!(
        "SELECT {key_value} AS entity \
         FROM {relation_identifier} \
         WHERE {key_identifier} IS NOT NULL AND {key_trimmed} <> '' \
         GROUP BY entity \
         HAVING COUNT(*) > 1 \
         ORDER BY entity \
         LIMIT {DUPLICATE_SAMPLE_LIMIT}"
    );
    let mut statement = candidate.connection().prepare(&sql)?;
    let rows = statement.query_map([], |row| row.get(0))?;

    let mut sample_entities = Vec::new();
    for row in rows {
        sample_entities.push(row?);
    }
    Ok(sample_entities)
}

fn null_or_blank_row_count(
    candidate: &LoadedCandidate,
    relation_identifier: &str,
    key_identifier: &str,
    key_trimmed: &str,
) -> duckdb::Result<i64> {
    let sql = format!(
        "SELECT COUNT(*) \
         FROM {relation_identifier} \
         WHERE {key_identifier} IS NULL OR {key_trimmed} = ''"
    );
    candidate.connection().query_row(&sql, [], |row| row.get(0))
}

fn benchmarked_row_count(
    candidate: &LoadedCandidate,
    relation_identifier: &str,
) -> duckdb::Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM {relation_identifier}");
    candidate.connection().query_row(&sql, [], |row| row.get(0))
}

fn row_count_to_usize(key_column: &str, row_count: i64) -> Result<usize, KeyCheckError> {
    usize::try_from(row_count).map_err(|_| KeyCheckError::RowCountOverflow {
        key_column: key_column.to_owned(),
        row_count,
    })
}

fn sql_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        error::Error,
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use crate::candidate::LoadedCandidate;

    use super::{E_KEY_NOT_FOUND, E_KEY_NOT_UNIQUE, E_KEY_NULL, KeyCheckError, validate_key};

    static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempCandidate {
        path: PathBuf,
    }

    impl TempCandidate {
        fn write(name: &str, contents: &str) -> std::io::Result<Self> {
            let unique = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "benchmark-key-check-{}-{unique}-{name}",
                std::process::id()
            ));
            fs::write(&path, contents)?;
            Ok(Self { path })
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempCandidate {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    #[allow(non_snake_case)]
    #[test]
    fn BENCH_U010_missing_key_column_refuses_with_e_key_not_found() -> Result<(), Box<dyn Error>> {
        let candidate = TempCandidate::write(
            "candidate.csv",
            "entity_id,property_name\ncomp_1,Marquis at Briarcliff\n",
        )?;
        let loaded = LoadedCandidate::load(candidate.path())?;

        let error = match validate_key(&loaded, "comp_id") {
            Ok(result) => return Err(format!("unexpectedly validated key: {result:?}").into()),
            Err(error) => error,
        };

        assert_eq!(error.refusal_code(), E_KEY_NOT_FOUND);
        assert!(matches!(
            error,
            KeyCheckError::KeyNotFound {
                ref available_columns,
                ..
            } if available_columns == &vec!["entity_id".to_owned(), "property_name".to_owned()]
        ));
        assert_eq!(error.refusal_detail()["key_column"], "comp_id");
        Ok(())
    }

    #[allow(non_snake_case)]
    #[test]
    fn BENCH_U011_duplicate_key_values_refuse_with_e_key_not_unique() -> Result<(), Box<dyn Error>>
    {
        let candidate = TempCandidate::write(
            "candidate.csv",
            "comp_id,property_name\ncomp_1,Marquis at Briarcliff\ncomp_1,Briarcliff Commons\ncomp_2,Peachtree Pointe\n",
        )?;
        let loaded = LoadedCandidate::load(candidate.path())?;

        let error = match validate_key(&loaded, "comp_id") {
            Ok(result) => return Err(format!("unexpectedly validated key: {result:?}").into()),
            Err(error) => error,
        };

        assert_eq!(error.refusal_code(), E_KEY_NOT_UNIQUE);
        assert!(matches!(
            error,
            KeyCheckError::KeyNotUnique {
                ref sample_entities,
                ..
            } if sample_entities == &vec!["comp_1".to_owned()]
        ));
        assert_eq!(error.refusal_detail()["sample_entities"][0], "comp_1");
        Ok(())
    }

    #[allow(non_snake_case)]
    #[test]
    fn BENCH_U012_null_or_blank_key_values_refuse_with_e_key_null() -> Result<(), Box<dyn Error>> {
        let candidate = TempCandidate::write(
            "candidate.jsonl",
            "{\"comp_id\":\"comp_1\",\"property_name\":\"Marquis at Briarcliff\"}\n{\"comp_id\":null,\"property_name\":\"Briarcliff Commons\"}\n{\"comp_id\":\"   \",\"property_name\":\"Peachtree Pointe\"}\n",
        )?;
        let loaded = LoadedCandidate::load(candidate.path())?;

        let error = match validate_key(&loaded, "comp_id") {
            Ok(result) => return Err(format!("unexpectedly validated key: {result:?}").into()),
            Err(error) => error,
        };

        assert_eq!(error.refusal_code(), E_KEY_NULL);
        assert!(matches!(error, KeyCheckError::KeyNull { null_rows: 2, .. }));
        assert_eq!(error.refusal_detail()["null_rows"], 2);
        Ok(())
    }

    #[allow(non_snake_case)]
    #[test]
    fn BENCH_U013_valid_key_returns_benchmarked_row_count() -> Result<(), Box<dyn Error>> {
        let candidate = TempCandidate::write(
            "candidate.csv",
            "comp_id,property_name\ncomp_1,Marquis at Briarcliff\ncomp_2,Briarcliff Commons\n",
        )?;
        let loaded = LoadedCandidate::load(candidate.path())?;

        let result = validate_key(&loaded, "comp_id")?;

        assert_eq!(result.key_column, "comp_id");
        assert_eq!(result.benchmarked_rows, 2);
        Ok(())
    }
}
