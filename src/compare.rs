use std::str::FromStr;

use bigdecimal::{BigDecimal, Signed};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use thiserror::Error;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareAs {
    #[default]
    String,
    Number,
    Percent,
    Date,
}

impl CompareAs {
    pub const fn label(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Percent => "percent",
            Self::Date => "date",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonOutcome {
    pub matched: bool,
    pub expected: String,
    pub actual: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CompareError {
    #[error("tolerance is only allowed for number and percent comparisons, not {compare_as}")]
    InvalidTolerance { compare_as: &'static str },
    #[error("tolerance must be non-negative, got {value}")]
    NegativeTolerance { value: String },
    #[error("tolerance must be a bounded finite decimal, got {value}")]
    InvalidNumericTolerance { value: String },
    #[error("failed to parse number value '{value}'")]
    InvalidNumber { value: String },
    #[error("failed to parse percent value '{value}'")]
    InvalidPercent { value: String },
    #[error("failed to parse canonical date value '{value}'")]
    InvalidDate { value: String },
}

pub fn compare_values(
    expected: &str,
    actual: &str,
    compare_as: CompareAs,
    tolerance: Option<&Number>,
) -> Result<ComparisonOutcome, CompareError> {
    let expected = expected.trim();
    let actual = actual.trim();

    match compare_as {
        CompareAs::String => {
            reject_tolerance(compare_as, tolerance)?;
            Ok(ComparisonOutcome {
                matched: expected == actual,
                expected: expected.to_owned(),
                actual: actual.to_owned(),
            })
        }
        CompareAs::Number => compare_number(expected, actual, tolerance),
        CompareAs::Percent => compare_percent(expected, actual, tolerance),
        CompareAs::Date => {
            reject_tolerance(compare_as, tolerance)?;
            let expected = normalize_date(expected)?;
            let actual = normalize_date(actual)?;

            Ok(ComparisonOutcome {
                matched: expected == actual,
                expected,
                actual,
            })
        }
    }
}

fn compare_number(
    expected: &str,
    actual: &str,
    tolerance: Option<&Number>,
) -> Result<ComparisonOutcome, CompareError> {
    let tolerance = numeric_tolerance(tolerance)?;
    let expected_value = parse_number(expected)?;
    let actual_value = parse_number(actual)?;
    let matched = (expected_value - actual_value).abs() <= tolerance;

    Ok(ComparisonOutcome {
        matched,
        expected: expected.to_owned(),
        actual: actual.to_owned(),
    })
}

fn compare_percent(
    expected: &str,
    actual: &str,
    tolerance: Option<&Number>,
) -> Result<ComparisonOutcome, CompareError> {
    let tolerance = numeric_tolerance(tolerance)?;
    let expected_value = parse_percent(expected)?;
    let actual_value = parse_percent(actual)?;
    let matched = (expected_value - actual_value).abs() <= tolerance;

    Ok(ComparisonOutcome {
        matched,
        expected: expected.to_owned(),
        actual: actual.to_owned(),
    })
}

fn reject_tolerance(compare_as: CompareAs, tolerance: Option<&Number>) -> Result<(), CompareError> {
    if tolerance.is_some() {
        return Err(CompareError::InvalidTolerance {
            compare_as: compare_as.label(),
        });
    }

    Ok(())
}

pub(crate) fn numeric_tolerance(tolerance: Option<&Number>) -> Result<BigDecimal, CompareError> {
    let Some(token) = tolerance else {
        return Ok(BigDecimal::default());
    };
    let text = token.to_string();
    let value = bounded_decimal(&text).ok_or_else(|| CompareError::InvalidNumericTolerance {
        value: text.clone(),
    })?;
    if value.is_negative() {
        return Err(CompareError::NegativeTolerance { value: text });
    }
    Ok(value)
}

fn bounded_decimal(text: &str) -> Option<BigDecimal> {
    // Bound exponent alignment before exact subtraction can allocate large
    // coefficients. This is a resource limit, never a rounding precision.
    if text.len() > 4096
        || !text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.' | b'e' | b'E'))
    {
        return None;
    }
    let value = BigDecimal::from_str(text).ok()?;
    // Check before normalization too: an extreme i64 scale must not overflow
    // while removing trailing zeros from the coefficient.
    if value.as_bigint_and_exponent().1.unsigned_abs() > 4096 {
        return None;
    }
    let value = value.normalized();
    let (coefficient, scale) = value.as_bigint_and_exponent();
    (scale.unsigned_abs() <= 1024
        && coefficient.to_str_radix(10).trim_start_matches('-').len() <= 1024)
        .then_some(value)
}

fn parse_number(value: &str) -> Result<BigDecimal, CompareError> {
    bounded_decimal(value).ok_or_else(|| CompareError::InvalidNumber {
        value: value.to_owned(),
    })
}

fn parse_percent(value: &str) -> Result<BigDecimal, CompareError> {
    let stripped = value
        .strip_suffix('%')
        .ok_or_else(|| CompareError::InvalidPercent {
            value: value.to_owned(),
        })?;

    bounded_decimal(stripped.trim()).ok_or_else(|| CompareError::InvalidPercent {
        value: value.to_owned(),
    })
}

fn normalize_date(value: &str) -> Result<String, CompareError> {
    let separator = ['-', '/', '.']
        .into_iter()
        .find(|separator| value.contains(*separator))
        .ok_or_else(|| CompareError::InvalidDate {
            value: value.to_owned(),
        })?;

    let parts: Vec<_> = value.split(separator).collect();
    if parts.len() != 3 || parts.iter().any(|part| part.trim().is_empty()) {
        return Err(CompareError::InvalidDate {
            value: value.to_owned(),
        });
    }

    if parts[0].trim().len() != 4 {
        return Err(CompareError::InvalidDate {
            value: value.to_owned(),
        });
    }

    let year = parse_date_component(parts[0], value)?;
    let month = parse_date_component(parts[1], value)?;
    let day = parse_date_component(parts[2], value)?;

    if month == 0 || month > 12 {
        return Err(CompareError::InvalidDate {
            value: value.to_owned(),
        });
    }

    if day == 0 || day > days_in_month(year, month) {
        return Err(CompareError::InvalidDate {
            value: value.to_owned(),
        });
    }

    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

fn parse_date_component(value: &str, original: &str) -> Result<u32, CompareError> {
    value
        .trim()
        .parse::<u32>()
        .map_err(|_| CompareError::InvalidDate {
            value: original.to_owned(),
        })
}

const fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

const fn is_leap_year(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::{CompareAs, CompareError, compare_values};

    #[test]
    fn BENCH_U003_number_honors_absolute_tolerance() -> Result<(), Box<dyn std::error::Error>> {
        let within_tolerance = compare_values(
            "28200000",
            "28200500",
            CompareAs::Number,
            Some(&serde_json::Number::from(1000)),
        )?;
        let outside_tolerance = compare_values(
            "28200000",
            "28202000",
            CompareAs::Number,
            Some(&serde_json::Number::from(1000)),
        )?;

        assert!(within_tolerance.matched);
        assert!(!outside_tolerance.matched);
        assert_eq!(outside_tolerance.expected, "28200000");
        assert_eq!(outside_tolerance.actual, "28202000");

        Ok(())
    }

    #[test]
    fn BENCH_U004_percent_does_not_auto_convert_ratio_form_decimals() {
        let error = compare_values("6.76%", "0.0676", CompareAs::Percent, None).unwrap_err();

        assert_eq!(
            error,
            CompareError::InvalidPercent {
                value: "0.0676".to_owned(),
            }
        );
    }

    #[test]
    fn BENCH_U005_date_normalizes_canonical_text() -> Result<(), Box<dyn std::error::Error>> {
        let outcome = compare_values("2026-03-08", "2026/3/8", CompareAs::Date, None)?;

        assert!(outcome.matched);
        assert_eq!(outcome.expected, "2026-03-08");
        assert_eq!(outcome.actual, "2026-03-08");

        Ok(())
    }

    #[test]
    fn BENCH_U009_string_trims_whitespace() -> Result<(), Box<dyn std::error::Error>> {
        let outcome = compare_values(
            " Marquis at Briarcliff ",
            "Marquis at Briarcliff",
            CompareAs::String,
            None,
        )?;

        assert!(outcome.matched);
        assert_eq!(outcome.expected, "Marquis at Briarcliff");
        assert_eq!(outcome.actual, "Marquis at Briarcliff");

        Ok(())
    }

    #[test]
    fn BENCH_U010_number_parse_does_not_fallback_to_string() {
        let error =
            compare_values("not-a-number", "not-a-number", CompareAs::Number, None).unwrap_err();

        assert_eq!(
            error,
            CompareError::InvalidNumber {
                value: "not-a-number".to_owned(),
            }
        );
    }

    #[test]
    fn BENCH_U011_tolerance_is_rejected_for_string_and_date() {
        let string_error = compare_values(
            "value",
            "value",
            CompareAs::String,
            Some(&"0.1".parse().expect("fixture tolerance")),
        )
        .unwrap_err();
        let date_error = compare_values(
            "2026-03-08",
            "2026-03-08",
            CompareAs::Date,
            Some(&serde_json::Number::from(1)),
        )
        .unwrap_err();

        assert_eq!(
            string_error,
            CompareError::InvalidTolerance {
                compare_as: "string",
            }
        );
        assert_eq!(
            date_error,
            CompareError::InvalidTolerance { compare_as: "date" }
        );
    }

    #[test]
    fn BENCH_U012_negative_tolerance_is_rejected_for_numeric_modes() {
        let number_error = compare_values(
            "10",
            "10",
            CompareAs::Number,
            Some(&serde_json::Number::from(-1)),
        )
        .unwrap_err();
        let percent_error = compare_values(
            "5%",
            "5%",
            CompareAs::Percent,
            Some(&"-0.5".parse().expect("fixture tolerance")),
        )
        .unwrap_err();

        assert_eq!(
            number_error,
            CompareError::NegativeTolerance {
                value: "-1".to_owned(),
            }
        );
        assert_eq!(
            percent_error,
            CompareError::NegativeTolerance {
                value: "-0.5".to_owned(),
            }
        );
    }
}
