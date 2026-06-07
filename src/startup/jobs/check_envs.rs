//! Startup job: check that the active provider has a usable credential.
//!
//! Lightweight, no network. Logs a warning when no API key is configured so
//! the user gets an early hint, but never fails the boot.

use crate::startup::job::{StartupContext, StartupJob};
use async_trait::async_trait;

pub struct CheckEnvsJob;

#[async_trait]
impl StartupJob for CheckEnvsJob {
    fn name(&self) -> &'static str {
        "check-envs"
    }

    async fn run(&self, ctx: &StartupContext) -> anyhow::Result<()> {
        let (provider, _model) = ctx.config.providers.active_provider_and_model();

        if provider == "none" {
            tracing::warn!(
                "[startup] no active provider configured — set an API key or run `opencrabs onboard`"
            );
            return Ok(());
        }

        // CLI-backed providers carry no API key in config; only flag the
        // key-requiring ones.
        if !ctx.config.has_any_api_key() && !provider.starts_with("custom:") {
            tracing::warn!(
                "[startup] active provider '{}' has no API key in config or env",
                provider
            );
        } else {
            tracing::debug!("[startup] provider '{}' credential present", provider);
        }
        Ok(())
    }
}
