#![forbid(unsafe_code)]

use std::{io::Read, process::ExitCode};

use benchmark::{Outcome, cli::Cli};
use clap::{Parser, error::ErrorKind};
use gag::BufferRedirect;

fn main() -> ExitCode {
    let raw_args = std::env::args().collect::<Vec<_>>();
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return handle_cli_error(error, &raw_args),
    };
    let stderr_capture = StderrCapture::start();
    let result = benchmark::execute(cli);
    let captured_stderr = stderr_capture.finish();

    match result {
        Ok(execution) => {
            if execution.outcome == Outcome::Refusal {
                replay_stderr(&captured_stderr);
            }
            print!("{}", execution.stdout);
            ExitCode::from(execution.exit_code())
        }
        Err(error) => {
            replay_stderr(&captured_stderr);
            eprintln!("{error}");
            eprintln!("next: benchmark capabilities --json");
            eprintln!("help: benchmark robot-docs guide");
            ExitCode::from(2)
        }
    }
}

fn handle_cli_error(error: clap::Error, raw_args: &[String]) -> ExitCode {
    match error.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            print!("{error}");
            ExitCode::SUCCESS
        }
        _ => {
            eprint!("{error}");
            if raw_args.iter().any(|arg| is_json_typo(arg)) {
                eprintln!("hint: did you mean `--json`?");
            }
            eprintln!("next: benchmark capabilities --json");
            eprintln!("help: benchmark robot-docs guide");
            ExitCode::from(2)
        }
    }
}

fn is_json_typo(arg: &str) -> bool {
    matches!(arg, "--jsno" | "--jsson" | "--jason" | "--josn")
}

struct StderrCapture {
    redirect: Option<BufferRedirect>,
}

impl StderrCapture {
    fn start() -> Self {
        Self {
            // DuckDB/native dependencies can emit benign stderr warnings directly
            // during otherwise successful scoring runs. Capture them here so the
            // CLI can keep normal PASS/FAIL operator output clean.
            redirect: BufferRedirect::stderr().ok(),
        }
    }

    fn finish(self) -> String {
        let Some(redirect) = self.redirect else {
            return String::new();
        };

        let mut buffer = redirect.into_inner();
        let mut captured = String::new();
        let _ = buffer.read_to_string(&mut captured);
        captured
    }
}

fn replay_stderr(captured: &str) {
    if !captured.is_empty() {
        eprint!("{captured}");
    }
}
