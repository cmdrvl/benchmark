//! Ambient witness ledger: one JSONL record per scoring run.
//!
//! The ledger lives at `$EPISTEMIC_WITNESS` when set, else
//! `~/.cmdrvl/state/witness/witness.jsonl`. Recording is best effort: a failure
//! to write never changes the scoring outcome or exit code (R-017), and
//! `--no-witness` suppresses recording entirely (R-008). Records carry the same
//! fields as the other spine tools so `witness query` output is uniform.
//!
//! Queries follow the family exit-code rule: an empty result is success
//! (exit 0, `[]`), never exit 1.

use std::{
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    Execution, Outcome, TOOL,
    cli::{WitnessAction, WitnessCountArgs, WitnessQueryArgs},
};

pub const WITNESS_ENV: &str = "EPISTEMIC_WITNESS";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WitnessRecord {
    pub id: String,
    pub tool: String,
    pub version: String,
    pub binary_hash: String,
    pub inputs: Vec<WitnessInput>,
    pub params: Value,
    pub outcome: String,
    pub exit_code: u8,
    pub output_hash: String,
    pub ts: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WitnessInput {
    pub path: String,
    pub hash: String,
    pub bytes: u64,
}

/// What a scoring run was asked to do; captured before the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunInfo {
    pub candidate: PathBuf,
    pub assertions: PathBuf,
    pub key: String,
    pub lockfiles: Vec<PathBuf>,
}

/// Where the ledger is written and read.
pub fn ledger_path() -> io::Result<PathBuf> {
    ledger_path_from(|name| env::var_os(name))
}

fn ledger_path_from(get_env: impl Fn(&str) -> Option<OsString>) -> io::Result<PathBuf> {
    if let Some(path) = get_env(WITNESS_ENV).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let home = get_env("HOME")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "HOME is unset and EPISTEMIC_WITNESS is not set; cannot locate the witness ledger",
            )
        })?;
    Ok(PathBuf::from(home)
        .join(".cmdrvl")
        .join("state")
        .join("witness")
        .join("witness.jsonl"))
}

/// Record one scoring run. Callers must treat an `Err` as a warning only.
pub fn record_run(info: &RunInfo, execution: &Execution) -> io::Result<()> {
    append_record(&ledger_path()?, &build_record(info, execution))
}

pub fn build_record(info: &RunInfo, execution: &Execution) -> WitnessRecord {
    let mut inputs = Vec::new();
    for path in [&info.candidate, &info.assertions]
        .into_iter()
        .chain(info.lockfiles.iter())
    {
        // An unreadable input (e.g. the E_IO refusal itself) is simply not recorded.
        if let Ok(bytes) = fs::read(path) {
            inputs.push(WitnessInput {
                path: path.to_string_lossy().into_owned(),
                hash: format!("blake3:{}", blake3::hash(&bytes).to_hex()),
                bytes: bytes.len() as u64,
            });
        }
    }

    let mut record = WitnessRecord {
        id: String::new(),
        tool: TOOL.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        binary_hash: binary_hash(),
        inputs,
        params: json!({
            "key": info.key,
            "lock": info.lockfiles.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>(),
        }),
        outcome: outcome_label(execution.outcome).to_owned(),
        exit_code: execution.exit_code(),
        output_hash: format!(
            "blake3:{}",
            blake3::hash(execution.stdout.as_bytes()).to_hex()
        ),
        ts: timestamp(),
    };
    record.id = record_id(&record);
    record
}

fn outcome_label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Pass => "PASS",
        Outcome::Fail => "FAIL",
        Outcome::Refusal => "REFUSAL",
    }
}

fn canonical_json(record: &WitnessRecord) -> String {
    serde_json::to_string(record).unwrap_or_default()
}

/// Content-addressed id: BLAKE3 of the canonical record with `id` blanked.
fn record_id(record: &WitnessRecord) -> String {
    let mut blanked = record.clone();
    blanked.id.clear();
    format!(
        "blake3:{}",
        blake3::hash(canonical_json(&blanked).as_bytes()).to_hex()
    )
}

fn binary_hash() -> String {
    env::current_exe()
        .ok()
        .and_then(|exe| fs::read(exe).ok())
        .map(|bytes| format!("blake3:{}", blake3::hash(&bytes).to_hex()))
        .unwrap_or_default()
}

pub fn append_record(path: &Path, record: &WitnessRecord) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", canonical_json(record))?;
    file.flush()?;
    file.sync_all()
}

/// UTC timestamp; honors `SOURCE_DATE_EPOCH` so output is reproducible.
fn timestamp() -> String {
    let seconds = env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs())
                .unwrap_or_default()
        });
    iso8601(seconds)
}

fn iso8601(total_seconds: u64) -> String {
    let days = (total_seconds / 86_400) as i64;
    let time_of_day = total_seconds % 86_400;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time_of_day / 3_600,
        (time_of_day % 3_600) / 60,
        time_of_day % 60
    )
}

// ---- witness query / last / count -----------------------------------------

pub fn execute(action: &WitnessAction, json: bool) -> io::Result<Execution> {
    let records = load_records(&ledger_path()?)?;
    Ok(execute_with_records(action, &records, json))
}

fn load_records(path: &Path) -> io::Result<Vec<WitnessRecord>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut records = Vec::new();
    for line in io::BufReader::new(file).lines() {
        let line = line?;
        if let Ok(record) = serde_json::from_str::<WitnessRecord>(line.trim()) {
            records.push(record);
        }
    }
    Ok(records)
}

fn execute_with_records(
    action: &WitnessAction,
    records: &[WitnessRecord],
    json: bool,
) -> Execution {
    match action {
        WitnessAction::Query(args) => {
            let matched = query_matches(args, records);
            if json {
                json_execution(&matched)
            } else {
                text_execution(format_records_human(&matched))
            }
        }
        WitnessAction::Last => match records.last() {
            Some(record) if json => json_execution(record),
            Some(record) => text_execution(format_record_human(record)),
            None if json => json_execution(&Vec::<WitnessRecord>::new()),
            None => text_execution(String::new()),
        },
        WitnessAction::Count(args) => {
            let count = records.iter().filter(|r| count_filter(args, r)).count();
            if json {
                json_execution(&json!({ "count": count }))
            } else {
                text_execution(count.to_string())
            }
        }
    }
}

fn json_execution<T: Serialize>(value: &T) -> Execution {
    let mut rendered = serde_json::to_string(value).unwrap_or_else(|_| "[]".to_owned());
    rendered.push('\n');
    Execution::new(Outcome::Pass, rendered)
}

fn text_execution(text: String) -> Execution {
    if text.is_empty() {
        Execution::new(Outcome::Pass, "")
    } else {
        Execution::new(Outcome::Pass, format!("{text}\n"))
    }
}

struct Filter<'a> {
    tool: Option<&'a str>,
    since: Option<&'a str>,
    until: Option<&'a str>,
    outcome: Option<&'a str>,
    input_hash: Option<&'a str>,
}

impl Filter<'_> {
    fn matches(&self, record: &WitnessRecord) -> bool {
        self.tool.is_none_or(|tool| record.tool == tool)
            && self.since.is_none_or(|since| record.ts.as_str() >= since)
            && self.until.is_none_or(|until| record.ts.as_str() <= until)
            && self
                .outcome
                .is_none_or(|outcome| record.outcome.eq_ignore_ascii_case(outcome))
            && self
                .input_hash
                .is_none_or(|hash| record.inputs.iter().any(|input| input.hash.contains(hash)))
    }
}

fn query_matches(args: &WitnessQueryArgs, records: &[WitnessRecord]) -> Vec<WitnessRecord> {
    let filter = Filter {
        tool: args.tool.as_deref(),
        since: args.since.as_deref(),
        until: args.until.as_deref(),
        outcome: args.outcome.as_deref(),
        input_hash: args.input_hash.as_deref(),
    };
    records
        .iter()
        .filter(|record| filter.matches(record))
        .take(args.limit)
        .cloned()
        .collect()
}

fn count_filter(args: &WitnessCountArgs, record: &WitnessRecord) -> bool {
    Filter {
        tool: args.tool.as_deref(),
        since: args.since.as_deref(),
        until: args.until.as_deref(),
        outcome: args.outcome.as_deref(),
        input_hash: args.input_hash.as_deref(),
    }
    .matches(record)
}

fn format_record_human(record: &WitnessRecord) -> String {
    let mut lines = vec![
        format!("id:       {}", record.id),
        format!("ts:       {}", record.ts),
        format!("tool:     {}", record.tool),
        format!("version:  {}", record.version),
        format!("outcome:  {}", record.outcome),
        format!("exit:     {}", record.exit_code),
    ];
    for (index, input) in record.inputs.iter().enumerate() {
        lines.push(format!(
            "input[{index}]: {} ({} bytes, {})",
            input.path, input.bytes, input.hash
        ));
    }
    lines.join("\n")
}

fn format_records_human(records: &[WitnessRecord]) -> String {
    records
        .iter()
        .map(format_record_human)
        .collect::<Vec<_>>()
        .join("\n---\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn ledger_path_prefers_env_then_home() {
        assert_eq!(
            ledger_path_from(env_of(&[(WITNESS_ENV, "/tmp/w.jsonl"), ("HOME", "/h")])).ok(),
            Some(PathBuf::from("/tmp/w.jsonl"))
        );
        assert_eq!(
            ledger_path_from(env_of(&[("HOME", "/h")])).ok(),
            Some(PathBuf::from("/h/.cmdrvl/state/witness/witness.jsonl"))
        );
        assert!(ledger_path_from(env_of(&[])).is_err());
    }

    #[test]
    fn iso8601_formats_known_epochs() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(iso8601(951_782_400), "2000-02-29T00:00:00Z");
    }

    fn record(tool: &str, outcome: &str, ts: &str, hash: &str) -> WitnessRecord {
        WitnessRecord {
            tool: tool.to_owned(),
            outcome: outcome.to_owned(),
            ts: ts.to_owned(),
            inputs: vec![WitnessInput {
                path: "c.csv".to_owned(),
                hash: hash.to_owned(),
                bytes: 1,
            }],
            ..WitnessRecord::default()
        }
    }

    #[test]
    fn filters_select_by_outcome_time_and_hash() {
        let records = vec![
            record("benchmark", "PASS", "2026-01-01T00:00:00Z", "blake3:aaa"),
            record("benchmark", "FAIL", "2026-02-01T00:00:00Z", "blake3:bbb"),
        ];
        let query = |outcome: Option<&str>, since: Option<&str>, hash: Option<&str>| {
            query_matches(
                &WitnessQueryArgs {
                    tool: None,
                    since: since.map(str::to_owned),
                    until: None,
                    outcome: outcome.map(str::to_owned),
                    input_hash: hash.map(str::to_owned),
                    limit: 20,
                },
                &records,
            )
        };
        assert_eq!(query(None, None, None).len(), 2);
        assert_eq!(query(Some("fail"), None, None).len(), 1);
        assert_eq!(query(None, Some("2026-01-15T00:00:00Z"), None).len(), 1);
        assert_eq!(query(None, None, Some("aaa")).len(), 1);
        assert!(query(Some("REFUSAL"), None, None).is_empty());
    }

    #[test]
    fn empty_results_are_success_and_print_empty_array() {
        let action = WitnessAction::Query(WitnessQueryArgs {
            tool: None,
            since: None,
            until: None,
            outcome: Some("REFUSAL".to_owned()),
            input_hash: None,
            limit: 20,
        });
        let execution = execute_with_records(&action, &[], true);
        assert_eq!(execution.exit_code(), 0);
        assert_eq!(execution.stdout.trim(), "[]");
    }

    #[test]
    fn record_id_is_content_addressed() {
        let mut first = record("benchmark", "PASS", "2026-01-01T00:00:00Z", "blake3:aaa");
        first.id = record_id(&first);
        let mut changed = first.clone();
        changed.outcome = "FAIL".to_owned();
        assert_eq!(first.id, record_id(&first), "id ignores its own field");
        assert_ne!(first.id, record_id(&changed));
    }

    #[test]
    fn append_is_additive_and_creates_parent_dirs() {
        let dir = env::temp_dir().join(format!("benchmark-witness-{}", std::process::id()));
        let path = dir.join("nested/witness.jsonl");
        append_record(&path, &record("benchmark", "PASS", "t1", "h")).expect("first append");
        append_record(&path, &record("benchmark", "FAIL", "t2", "h")).expect("second append");
        let records = load_records(&path).expect("ledger loads");
        assert_eq!(records.len(), 2);
        assert_eq!(records[1].outcome, "FAIL");
    }

    #[test]
    fn unwritable_ledger_is_an_error_not_a_panic() {
        let result = append_record(
            Path::new("/dev/null/impossible/witness.jsonl"),
            &record("benchmark", "PASS", "t", "h"),
        );
        assert!(result.is_err());
    }
}
