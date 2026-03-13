use serde::Serialize;
use serde_json::{Value, json};
use thiserror::Error;

use crate::{REPORT_VERSION, TOOL, cli::BenchmarkCommand, render::RenderMode};

#[derive(Debug, Clone, Serialize)]
pub struct RefusalEnvelope {
    tool: String,
    version: String,
    outcome: &'static str,
    policy_signals: RefusalPolicySignals,
    refusal: Refusal,
}

#[derive(Debug, Clone, Serialize)]
pub struct Refusal {
    code: String,
    message: String,
    detail: Value,
    next_command: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RefusalPolicySignals {}

#[derive(Debug, Error)]
pub enum RefusalError {
    #[error("failed to serialize refusal envelope: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl RefusalEnvelope {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        detail: Value,
        next_command: Option<String>,
    ) -> Self {
        Self {
            tool: TOOL.to_owned(),
            version: REPORT_VERSION.to_owned(),
            outcome: "REFUSAL",
            policy_signals: RefusalPolicySignals::default(),
            refusal: Refusal {
                code: code.into(),
                message: message.into(),
                detail,
                next_command,
            },
        }
    }

    pub fn render(&self, render_mode: RenderMode) -> Result<String, serde_json::Error> {
        match render_mode {
            RenderMode::Json => {
                let mut rendered = serde_json::to_string_pretty(self)?;
                rendered.push('\n');
                Ok(rendered)
            }
            RenderMode::Human => {
                let mut rendered = format!(
                    "REFUSAL [{}]\n{}\n",
                    self.refusal.code, self.refusal.message
                );
                if !self.refusal.detail.is_null() && self.refusal.detail != json!({}) {
                    rendered.push_str(&format!("detail: {}\n", self.refusal.detail));
                }
                if let Some(next_command) = &self.refusal.next_command {
                    rendered.push_str(&format!("next: {next_command}\n"));
                }
                Ok(rendered)
            }
            RenderMode::Summary => Ok(format!(
                "tool={} version={} candidate={} outcome={} accuracy={} coverage={} failed={} skipped={} quality_band={} refusal_code={}\n",
                self.tool,
                self.version,
                self.summary_candidate_label(),
                self.outcome,
                "-",
                "-",
                "-",
                "-",
                "-",
                self.refusal.code
            )),
            RenderMode::SummaryTsv => Ok(format!(
                "tool\tversion\tcandidate\toutcome\taccuracy\tcoverage\tfailed\tskipped\tquality_band\trefusal_code\n{}\t{}\t{}\t{}\t-\t-\t-\t-\t-\t{}\n",
                sanitize_tsv(self.tool.clone()),
                sanitize_tsv(self.version.clone()),
                sanitize_tsv(self.summary_candidate_label()),
                self.outcome,
                self.refusal.code
            )),
        }
    }

    fn summary_candidate_label(&self) -> String {
        self.refusal
            .detail
            .get("candidate")
            .and_then(Value::as_str)
            .or_else(|| self.refusal.detail.get("path").and_then(Value::as_str))
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| "-".to_owned())
    }
}

pub fn scaffold_only(command: &BenchmarkCommand) -> RefusalEnvelope {
    RefusalEnvelope::new(
        "E_NOT_IMPLEMENTED",
        "benchmark is scaffolded but scoring is not implemented yet.",
        json!({
            "candidate": command.candidate.display().to_string(),
            "assertions": command.assertions.display().to_string(),
            "key": command.key,
            "lockfiles": command
                .lockfiles
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
        }),
        Some("br ready".to_owned()),
    )
}

fn sanitize_tsv(value: String) -> String {
    value.replace('\t', " ").replace(['\n', '\r'], " ")
}
