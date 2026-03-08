use std::{
    io::Read,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    assertions::{Assertion, AssertionSet},
    candidate::LoadedCandidate,
    compare::{CompareAs, CompareError, compare_values},
    key_check::KeyCheckResult,
    lock_check::InputVerification,
    report::{AssertionOutcome, BenchmarkReport, EvaluatedAssertion, ReportContext},
};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("failed to evaluate assertions against the loaded candidate: {source}")]
    Query {
        #[source]
        source: duckdb::Error,
    },
    #[error("scoring projection index does not fit in usize: {index}")]
    ProjectionIndexOverflow { index: i64 },
    #[error("scoring projection returned {actual} rows for {expected} assertions")]
    ProjectionCardinality { expected: usize, actual: usize },
    #[error("failed to read scoring input '{path}': {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "assertion '{entity}.{field}' has an invalid expected value '{expected}' for compare_as={compare_as:?}: {source}"
    )]
    InvalidExpectedValue {
        entity: String,
        field: String,
        expected: String,
        compare_as: CompareAs,
        #[source]
        source: CompareError,
    },
    #[error(
        "assertion '{entity}.{field}' has an invalid comparison configuration for compare_as={compare_as:?}: {source}"
    )]
    InvalidComparisonConfiguration {
        entity: String,
        field: String,
        compare_as: CompareAs,
        #[source]
        source: CompareError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectionRow {
    entity_exists: bool,
    field_exists: bool,
    actual: Option<String>,
}

pub fn score_candidate(
    candidate: &LoadedCandidate,
    assertions_path: impl AsRef<Path>,
    assertions: &AssertionSet,
    key_check: &KeyCheckResult,
    input_verification: Option<InputVerification>,
) -> Result<BenchmarkReport, EngineError> {
    let assertions_path = assertions_path.as_ref();
    let evaluated = evaluate_assertions(candidate, assertions, key_check)?;

    Ok(BenchmarkReport::from_evaluated(
        ReportContext {
            candidate: candidate.source().path.display().to_string(),
            candidate_hash: hash_file(&candidate.source().path)?,
            assertions_file: assertions_path.display().to_string(),
            assertions_hash: hash_file(assertions_path)?,
            key_column: key_check.key_column.clone(),
            input_verification,
        },
        evaluated,
    ))
}

pub fn evaluate_assertions(
    candidate: &LoadedCandidate,
    assertions: &AssertionSet,
    key_check: &KeyCheckResult,
) -> Result<Vec<EvaluatedAssertion>, EngineError> {
    if assertions.is_empty() {
        return Ok(Vec::new());
    }

    let projections = project_candidate_values(candidate, assertions, &key_check.key_column)?;
    let mut evaluated = Vec::with_capacity(assertions.len());

    for (assertion, projection) in assertions.assertions.iter().zip(projections) {
        evaluated.push(evaluate_assertion(assertion, projection)?);
    }

    Ok(evaluated)
}

fn project_candidate_values(
    candidate: &LoadedCandidate,
    assertions: &AssertionSet,
    key_column: &str,
) -> Result<Vec<ProjectionRow>, EngineError> {
    let sql = build_projection_query(candidate, assertions, key_column);
    let mut statement = candidate
        .connection()
        .prepare(&sql)
        .map_err(|source| EngineError::Query { source })?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|source| EngineError::Query { source })?;

    let mut projections = vec![None; assertions.len()];

    for row in rows {
        let (index, entity_exists, field_exists, actual) =
            row.map_err(|source| EngineError::Query { source })?;
        let index =
            usize::try_from(index).map_err(|_| EngineError::ProjectionIndexOverflow { index })?;
        if index >= projections.len() {
            return Err(EngineError::ProjectionCardinality {
                expected: projections.len(),
                actual: index + 1,
            });
        }

        projections[index] = Some(ProjectionRow {
            entity_exists: entity_exists != 0,
            field_exists: field_exists != 0,
            actual,
        });
    }

    let actual_rows = projections.iter().filter(|row| row.is_some()).count();
    if actual_rows != assertions.len() {
        return Err(EngineError::ProjectionCardinality {
            expected: assertions.len(),
            actual: actual_rows,
        });
    }

    projections
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            row.ok_or(EngineError::ProjectionCardinality {
                expected: assertions.len(),
                actual: index,
            })
        })
        .collect()
}

fn build_projection_query(
    candidate: &LoadedCandidate,
    assertions: &AssertionSet,
    key_column: &str,
) -> String {
    let relation_identifier = sql_identifier(candidate.relation_name());
    let key_identifier = sql_identifier(key_column);
    let assertion_rows = assertions
        .assertions
        .iter()
        .enumerate()
        .map(|(index, assertion)| {
            format!(
                "({index}, '{}', '{}')",
                sql_string_literal(&assertion.entity),
                sql_string_literal(&assertion.field),
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let field_exists_cases = candidate
        .columns()
        .iter()
        .map(|column| format!("WHEN '{}' THEN 1", sql_string_literal(&column.name),))
        .collect::<Vec<_>>()
        .join(" ");
    let actual_cases = candidate
        .columns()
        .iter()
        .map(|column| {
            format!(
                "WHEN '{}' THEN CAST(c.{} AS VARCHAR)",
                sql_string_literal(&column.name),
                sql_identifier(&column.name),
            )
        })
        .collect::<Vec<_>>()
        .join(" ");

    format!(
        "WITH assertions(assertion_index, entity, field) AS (VALUES {assertion_rows}) \
         SELECT a.assertion_index, \
                CASE WHEN c.{key_identifier} IS NULL THEN 0 ELSE 1 END AS entity_exists, \
                CASE a.field {field_exists_cases} ELSE 0 END AS field_exists, \
                CASE a.field {actual_cases} ELSE NULL END AS actual \
         FROM assertions AS a \
         LEFT JOIN {relation_identifier} AS c \
           ON TRIM(CAST(c.{key_identifier} AS VARCHAR)) = a.entity \
         ORDER BY a.assertion_index"
    )
}

fn evaluate_assertion(
    assertion: &Assertion,
    projection: ProjectionRow,
) -> Result<EvaluatedAssertion, EngineError> {
    if !projection.field_exists {
        return Ok(EvaluatedAssertion {
            entity: assertion.entity.clone(),
            field: assertion.field.clone(),
            expected: assertion.expected.trim().to_owned(),
            actual: None,
            compare_as: assertion.compare_as,
            tolerance: assertion.tolerance,
            severity: assertion.severity,
            source: assertion.source.clone(),
            outcome: AssertionOutcome::SkipField,
            detail: Some(format!(
                "Field '{}' not found in candidate",
                assertion.field
            )),
        });
    }

    if !projection.entity_exists {
        return Ok(EvaluatedAssertion {
            entity: assertion.entity.clone(),
            field: assertion.field.clone(),
            expected: assertion.expected.trim().to_owned(),
            actual: None,
            compare_as: assertion.compare_as,
            tolerance: assertion.tolerance,
            severity: assertion.severity,
            source: assertion.source.clone(),
            outcome: AssertionOutcome::SkipEntity,
            detail: Some(format!(
                "Entity '{}' not found in candidate",
                assertion.entity
            )),
        });
    }

    let actual = projection.actual.map(|value| value.trim().to_owned());
    let actual_value = match actual.as_deref() {
        Some(value) => value,
        None => {
            return Ok(EvaluatedAssertion {
                entity: assertion.entity.clone(),
                field: assertion.field.clone(),
                expected: assertion.expected.trim().to_owned(),
                actual: None,
                compare_as: assertion.compare_as,
                tolerance: assertion.tolerance,
                severity: assertion.severity,
                source: assertion.source.clone(),
                outcome: AssertionOutcome::Fail,
                detail: None,
            });
        }
    };

    match compare_values(
        &assertion.expected,
        actual_value,
        assertion.compare_as,
        assertion.tolerance,
    ) {
        Ok(comparison) => Ok(EvaluatedAssertion {
            entity: assertion.entity.clone(),
            field: assertion.field.clone(),
            expected: comparison.expected,
            actual: Some(comparison.actual),
            compare_as: assertion.compare_as,
            tolerance: assertion.tolerance,
            severity: assertion.severity,
            source: assertion.source.clone(),
            outcome: if comparison.matched {
                AssertionOutcome::Pass
            } else {
                AssertionOutcome::Fail
            },
            detail: None,
        }),
        Err(error) if compare_error_targets_expected(&error, &assertion.expected, actual_value) => {
            Err(expected_value_error(assertion, error))
        }
        Err(error)
            if matches!(
                error,
                CompareError::InvalidTolerance { .. } | CompareError::NegativeTolerance { .. }
            ) =>
        {
            Err(EngineError::InvalidComparisonConfiguration {
                entity: assertion.entity.clone(),
                field: assertion.field.clone(),
                compare_as: assertion.compare_as,
                source: error,
            })
        }
        Err(_) => Ok(EvaluatedAssertion {
            entity: assertion.entity.clone(),
            field: assertion.field.clone(),
            expected: assertion.expected.trim().to_owned(),
            actual,
            compare_as: assertion.compare_as,
            tolerance: assertion.tolerance,
            severity: assertion.severity,
            source: assertion.source.clone(),
            outcome: AssertionOutcome::Fail,
            detail: None,
        }),
    }
}

fn compare_error_targets_expected(error: &CompareError, expected: &str, actual: &str) -> bool {
    match error {
        CompareError::InvalidNumber { value }
        | CompareError::InvalidPercent { value }
        | CompareError::InvalidDate { value } => value == expected.trim() || value != actual,
        CompareError::InvalidTolerance { .. } | CompareError::NegativeTolerance { .. } => false,
    }
}

fn expected_value_error(assertion: &Assertion, source: CompareError) -> EngineError {
    EngineError::InvalidExpectedValue {
        entity: assertion.entity.clone(),
        field: assertion.field.clone(),
        expected: assertion.expected.clone(),
        compare_as: assertion.compare_as,
        source,
    }
}

fn hash_file(path: &Path) -> Result<String, EngineError> {
    let mut file = std::fs::File::open(path).map_err(|source| EngineError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let read = file.read(&mut buffer).map_err(|source| EngineError::Io {
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

fn sql_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn sql_string_literal(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use crate::{
        assertions::{Assertion, AssertionSet, Severity},
        candidate::LoadedCandidate,
        engine::{AssertionOutcome, EngineError, evaluate_assertions},
        key_check::validate_key,
    };

    #[test]
    fn bench_u_engine_invalid_expected_number_refuses() -> Result<(), Box<dyn std::error::Error>> {
        let candidate = LoadedCandidate::load(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/candidates/smoke/bench_i001_candidate.csv"),
        )?;
        let key_check = validate_key(&candidate, "comp_id")?;
        let assertions = AssertionSet {
            assertions: vec![Assertion {
                entity: "comp_1".to_owned(),
                field: "cap_rate".to_owned(),
                expected: "not-a-percent".to_owned(),
                compare_as: crate::compare::CompareAs::Percent,
                tolerance: None,
                severity: Severity::Major,
                source: None,
            }],
        };

        let error = match evaluate_assertions(&candidate, &assertions, &key_check) {
            Ok(_) => return Err("invalid expected percent should refuse".into()),
            Err(error) => error,
        };

        assert!(matches!(error, EngineError::InvalidExpectedValue { .. }));
        Ok(())
    }

    #[test]
    fn bench_u_engine_invalid_actual_number_fails_without_refusal()
    -> Result<(), Box<dyn std::error::Error>> {
        let candidate_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/candidates/smoke/bench_i001_candidate.csv");
        let candidate = LoadedCandidate::load(candidate_path)?;
        let key_check = validate_key(&candidate, "comp_id")?;
        let assertions = AssertionSet {
            assertions: vec![Assertion {
                entity: "comp_1".to_owned(),
                field: "property_name".to_owned(),
                expected: "1".to_owned(),
                compare_as: crate::compare::CompareAs::Number,
                tolerance: None,
                severity: Severity::Major,
                source: None,
            }],
        };

        let evaluated = evaluate_assertions(&candidate, &assertions, &key_check)?;
        assert_eq!(evaluated.len(), 1);
        assert_eq!(evaluated[0].outcome, AssertionOutcome::Fail);
        assert_eq!(
            evaluated[0].actual.as_deref(),
            Some("Marquis at Briarcliff")
        );
        Ok(())
    }
}
