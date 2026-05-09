use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::render::RenderMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SummaryRenderMode {
    Summary,
    SummaryTsv,
}

#[derive(Debug, Clone, Parser)]
#[command(
    name = "benchmark",
    version,
    about = "Score a row-oriented candidate against human-validated assertions",
    subcommand_precedence_over_arg = true,
    subcommand_negates_reqs = true
)]
pub struct Cli {
    #[arg(
        value_name = "CANDIDATE",
        help = "File to score (CSV, JSON, JSONL, or Parquet)"
    )]
    pub candidate: Option<PathBuf>,

    #[arg(long, value_name = "FILE", help = "Assertion file (JSONL)")]
    pub assertions: Option<PathBuf>,

    #[arg(
        long,
        value_name = "COLUMN",
        help = "Key column for entity lookup in candidate"
    )]
    pub key: Option<String>,

    #[arg(
        long,
        value_name = "LOCKFILE",
        help = "Verify candidate is a member of these lockfiles (repeatable)"
    )]
    pub lock: Vec<PathBuf>,

    #[arg(long, global = true, help = "Emit machine-readable JSON output")]
    pub json: bool,

    #[arg(long, value_enum, conflicts_with = "json")]
    pub render: Option<SummaryRenderMode>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    Doctor(DoctorArgs),
}

#[derive(Debug, Clone, Args)]
pub struct DoctorArgs {
    #[arg(long = "robot-triage")]
    pub robot_triage: bool,

    #[command(subcommand)]
    pub command: Option<DoctorCommand>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum DoctorCommand {
    Health,
    Capabilities,
    #[command(name = "robot-docs")]
    RobotDocs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkCommand {
    pub candidate: PathBuf,
    pub assertions: PathBuf,
    pub key: String,
    pub lockfiles: Vec<PathBuf>,
    pub render_mode: RenderMode,
}

impl BenchmarkCommand {
    pub fn try_from_cli(cli: Cli) -> Result<Self, &'static str> {
        Ok(Self {
            candidate: cli
                .candidate
                .ok_or("missing required argument <CANDIDATE>")?,
            assertions: cli
                .assertions
                .ok_or("missing required argument --assertions <FILE>")?,
            key: cli.key.ok_or("missing required argument --key <COLUMN>")?,
            lockfiles: cli.lock,
            render_mode: match (cli.json, cli.render) {
                (true, _) => RenderMode::Json,
                (false, Some(SummaryRenderMode::Summary)) => RenderMode::Summary,
                (false, Some(SummaryRenderMode::SummaryTsv)) => RenderMode::SummaryTsv,
                (false, None) => RenderMode::Human,
            },
        })
    }
}
