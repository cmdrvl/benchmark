use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "benchmark",
    version,
    about = "Score a row-oriented candidate against human-validated assertions"
)]
pub struct Cli {
    #[arg(
        value_name = "CANDIDATE",
        help = "File to score (CSV, JSON, JSONL, or Parquet)"
    )]
    pub candidate: PathBuf,

    #[arg(long, value_name = "FILE", help = "Assertion file (JSONL)")]
    pub assertions: PathBuf,

    #[arg(
        long,
        value_name = "COLUMN",
        help = "Key column for entity lookup in candidate"
    )]
    pub key: String,

    #[arg(
        long,
        value_name = "LOCKFILE",
        help = "Verify candidate is a member of these lockfiles (repeatable)"
    )]
    pub lock: Vec<PathBuf>,

    #[arg(long, help = "Emit machine-readable JSON output")]
    pub json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkCommand {
    pub candidate: PathBuf,
    pub assertions: PathBuf,
    pub key: String,
    pub lockfiles: Vec<PathBuf>,
    pub json: bool,
}

impl From<Cli> for BenchmarkCommand {
    fn from(cli: Cli) -> Self {
        Self {
            candidate: cli.candidate,
            assertions: cli.assertions,
            key: cli.key,
            lockfiles: cli.lock,
            json: cli.json,
        }
    }
}
