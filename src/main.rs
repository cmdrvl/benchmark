#![forbid(unsafe_code)]

use std::{io::Read, process::ExitCode};

use benchmark::{Outcome, cli::Cli};
use clap::Parser;
use gag::BufferRedirect;

fn main() -> ExitCode {
    let cli = Cli::parse();
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
            ExitCode::from(2)
        }
    }
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
