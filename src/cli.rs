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
    long_about = "Score one row-oriented candidate relation against a human-validated JSONL assertion set.",
    after_long_help = "Agent entrypoints:\n  benchmark --robot-triage\n  benchmark capabilities --json\n  benchmark robot-docs guide\n\nExamples:\n  benchmark tests/fixtures/candidates/smoke/bench_i001_candidate.csv --assertions tests/fixtures/assertions/smoke/bench_i001_gold.jsonl --key comp_id --json\n  benchmark tests/fixtures/candidates/smoke/bench_mixed.csv --assertions tests/fixtures/assertions/smoke/bench_mixed_gold.jsonl --key comp_id --render summary\n  benchmark candidate.csv --assertions gold.jsonl --profile profile.json --json\n  benchmark capabilities --json\n  benchmark robot-docs guide",
    subcommand_precedence_over_arg = true,
    subcommand_negates_reqs = true
)]
pub struct Cli {
    #[arg(
        value_name = "CANDIDATE",
        help = "File to score (CSV, JSON, JSONL, Parquet, XLSX, XLS, or XLSM)"
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
        value_name = "NAME|INDEX",
        help = "Worksheet name or zero-based index for XLSX/XLS candidates"
    )]
    pub sheet: Option<String>,

    #[arg(
        long,
        value_name = "FILE",
        conflicts_with = "key",
        help = "Frozen JSON profile containing a key or key_columns string array"
    )]
    pub profile: Option<PathBuf>,

    #[arg(
        long,
        help = "Include raw expected and actual cell values in failure output"
    )]
    pub explicit: bool,

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

    #[arg(
        long = "robot-triage",
        help = "Emit compact read-only triage JSON for automation"
    )]
    pub robot_triage: bool,

    #[arg(
        long,
        help = "Print the operator manifest (same document as `capabilities --json`) and exit"
    )]
    pub describe: bool,

    #[arg(long, help = "Print the JSON Schema for benchmark.v0 stdout and exit")]
    pub schema: bool,

    #[arg(
        long = "no-witness",
        help = "Do not record this run in the ambient witness ledger"
    )]
    pub no_witness: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    Doctor(DoctorArgs),
    Capabilities,
    #[command(name = "robot-docs")]
    RobotDocs(RobotDocsArgs),
    /// Query the ambient witness ledger (`--json` for JSON output)
    Witness {
        #[command(subcommand)]
        action: WitnessAction,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum WitnessAction {
    /// List matching records, oldest first
    Query(WitnessQueryArgs),
    /// Show the most recent record
    Last,
    /// Count matching records
    Count(WitnessCountArgs),
}

#[derive(Debug, Clone, Args)]
pub struct WitnessQueryArgs {
    #[arg(long)]
    pub tool: Option<String>,
    #[arg(long, help = "Only records at or after this UTC timestamp")]
    pub since: Option<String>,
    #[arg(long, help = "Only records at or before this UTC timestamp")]
    pub until: Option<String>,
    #[arg(long, help = "PASS, FAIL or REFUSAL")]
    pub outcome: Option<String>,
    #[arg(long = "input-hash", help = "Substring of an input hash")]
    pub input_hash: Option<String>,
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
}

#[derive(Debug, Clone, Args)]
pub struct WitnessCountArgs {
    #[arg(long)]
    pub tool: Option<String>,
    #[arg(long)]
    pub since: Option<String>,
    #[arg(long)]
    pub until: Option<String>,
    #[arg(long)]
    pub outcome: Option<String>,
    #[arg(long = "input-hash")]
    pub input_hash: Option<String>,
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
    Triage,
    #[command(name = "robot-docs")]
    RobotDocs,
}

#[derive(Debug, Clone, Args)]
pub struct RobotDocsArgs {
    #[command(subcommand)]
    pub command: Option<RobotDocsCommand>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum RobotDocsCommand {
    Guide,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkCommand {
    pub candidate: PathBuf,
    pub assertions: PathBuf,
    pub key: String,
    pub sheet: Option<String>,
    pub profile: Option<PathBuf>,
    pub explicit: bool,
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
            key: cli.key.unwrap_or_default(),
            sheet: cli.sheet,
            profile: cli.profile,
            explicit: cli.explicit,
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
