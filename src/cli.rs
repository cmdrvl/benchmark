use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(
    name = "benchmark",
    version,
    about = "Score a row-oriented candidate against human-validated assertions"
)]
pub struct Cli {
    #[arg(value_name = "CANDIDATE")]
    pub candidate: PathBuf,

    #[arg(long, value_name = "FILE")]
    pub assertions: PathBuf,

    #[arg(long, value_name = "COLUMN")]
    pub key: String,

    #[arg(long, value_name = "LOCKFILE")]
    pub lock: Vec<PathBuf>,

    #[arg(long)]
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
