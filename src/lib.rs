#![forbid(unsafe_code)]

pub mod assertions;
pub mod candidate;
pub mod cli;
pub mod compare;
pub mod engine;
pub mod key_check;
pub mod lock_check;
pub mod refusal;
pub mod render;
pub mod report;

use cli::{BenchmarkCommand, Cli};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    pub exit_code: u8,
    pub stdout: String,
}

pub fn execute(cli: Cli) -> Result<Execution, Box<dyn std::error::Error>> {
    let command = BenchmarkCommand::from(cli);
    let refusal = refusal::scaffold_only(&command);
    let stdout = render::render_refusal(&refusal, command.json)?;

    Ok(Execution {
        exit_code: 2,
        stdout,
    })
}

#[cfg(test)]
mod tests {
    use crate::cli::Cli;

    use super::execute;

    #[test]
    fn scaffold_returns_refusal_shell() -> Result<(), Box<dyn std::error::Error>> {
        let cli = Cli {
            candidate: "candidate.csv".into(),
            assertions: "gold.jsonl".into(),
            key: "comp_id".to_owned(),
            lock: Vec::new(),
            json: true,
        };

        let execution = execute(cli)?;
        assert_eq!(execution.exit_code, 2);

        let json: serde_json::Value = serde_json::from_str(&execution.stdout)?;
        assert_eq!(json["outcome"], "REFUSAL");
        assert_eq!(json["refusal"]["code"], "E_NOT_IMPLEMENTED");
        Ok(())
    }
}
