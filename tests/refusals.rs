use std::{
    env, fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use benchmark::assertions::{
    AssertionError, E_BAD_ASSERTIONS, E_EMPTY_ASSERTIONS, Severity, load_assertions,
};
use benchmark::compare::CompareAs;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn write_temp_assertions(contents: &str) -> std::io::Result<PathBuf> {
    let unique = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = env::temp_dir().join(format!(
        "benchmark-refusals-{}-{unique}.jsonl",
        std::process::id()
    ));
    fs::write(&path, contents)?;
    Ok(path)
}

#[allow(non_snake_case)]
#[test]
fn BENCH_U014_public_loader_preserves_defaults_for_valid_assertions()
-> Result<(), Box<dyn std::error::Error>> {
    let path = write_temp_assertions(
        "{\"entity\":\"comp_1\",\"field\":\"u8:name\",\"expected\":\"Marquis\"}\n",
    )?;

    let set = load_assertions(&path)?;
    assert_eq!(set.assertions.len(), 1);
    assert_eq!(set.assertions[0].compare_as, CompareAs::String);
    assert_eq!(set.assertions[0].severity, Severity::Major);

    fs::remove_file(path)?;
    Ok(())
}

#[allow(non_snake_case)]
#[test]
fn BENCH_U015_public_loader_maps_semantic_failures_to_bad_assertions()
-> Result<(), Box<dyn std::error::Error>> {
    let path = write_temp_assertions(
        "{\"entity\":\"comp_1\",\"field\":\"u8:cap_rate\",\"expected\":\"6.76%\",\"compare_as\":\"date\",\"tolerance\":0.01}\n",
    )?;

    let error = match load_assertions(&path) {
        Ok(_) => return Err("illegal tolerance mode should refuse".into()),
        Err(error) => error,
    };
    assert_eq!(error.refusal_code(), E_BAD_ASSERTIONS);
    assert!(matches!(error, AssertionError::Semantic { line: 1, .. }));

    fs::remove_file(path)?;
    Ok(())
}

#[allow(non_snake_case)]
#[test]
fn BENCH_U016_public_loader_maps_blank_input_to_empty_assertions()
-> Result<(), Box<dyn std::error::Error>> {
    let path = write_temp_assertions("")?;

    let error = match load_assertions(&path) {
        Ok(_) => return Err("empty assertions file should refuse".into()),
        Err(error) => error,
    };
    assert_eq!(error.refusal_code(), E_EMPTY_ASSERTIONS);
    assert!(matches!(error, AssertionError::Empty { .. }));

    fs::remove_file(path)?;
    Ok(())
}
