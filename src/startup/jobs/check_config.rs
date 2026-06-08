//! Startup job: validate the loaded configuration.

use crate::startup::job::{StartupContext, StartupJob};
use async_trait::async_trait;

pub struct CheckConfigJob;

#[async_trait]
impl StartupJob for CheckConfigJob {
    fn name(&self) -> &'static str {
        "check-config"
    }

    async fn run(&self, ctx: &StartupContext) -> anyhow::Result<Option<String>> {
        ctx.config.validate()?;

        // Surface any unknown top-level keys collected during config load
        // (possible typos). Draining here folds them into the startup-info
        // line instead of a separate standalone system message.
        let typos = crate::config::Config::take_typo_warnings();
        if !typos.is_empty() {
            tracing::warn!("[startup] config: unknown keys: {}", typos.join(", "));
            return Ok(Some(format!("unknown keys: {}", typos.join(", "))));
        }

        tracing::debug!("[startup] config validated");
        Ok(Some("config valid".to_string()))
    }
}
