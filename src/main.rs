#![forbid(unsafe_code)]

use std::process::ExitCode;

use benchmark::cli::Cli;
use clap::Parser;

fn main() -> ExitCode {
    let cli = Cli::parse();

    match benchmark::execute(cli) {
        Ok(execution) => {
            print!("{}", execution.stdout);
            ExitCode::from(execution.exit_code)
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}
