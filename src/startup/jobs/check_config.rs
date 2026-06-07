//! Startup job: validate the loaded configuration.

use crate::startup::job::{StartupContext, StartupJob};
use async_trait::async_trait;

pub struct CheckConfigJob;

#[async_trait]
impl StartupJob for CheckConfigJob {
    fn name(&self) -> &'static str {
        "check-config"
    }

    async fn run(&self, ctx: &StartupContext) -> anyhow::Result<()> {
        ctx.config.validate()?;
        tracing::debug!("[startup] config validated");
        Ok(())
    }
}
