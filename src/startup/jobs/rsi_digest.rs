//! Startup job: write the RSI feedback digest at boot and report its size.
//!
//! Owns the one-time boot digest write that previously lived in
//! `spawn_rsi_engine` (behind a hardcoded 5s sleep). Running it here means the
//! digest is written promptly and its total event count is folded into the
//! startup-info line instead of a separate, delayed chat message. The RSI
//! engine still refreshes the digest on each periodic analysis cycle.

use crate::startup::job::{StartupContext, StartupJob};
use async_trait::async_trait;

pub struct RsiDigestJob;

#[async_trait]
impl StartupJob for RsiDigestJob {
    fn name(&self) -> &'static str {
        "rsi-digest"
    }

    async fn run(&self, ctx: &StartupContext) -> anyhow::Result<Option<String>> {
        let Some(pool) = ctx.pool.clone() else {
            return Ok(Some("skipped (no db)".to_string()));
        };

        crate::brain::rsi::write_startup_digest(pool.clone()).await;

        let repo = crate::db::repository::FeedbackLedgerRepository::new(pool);
        let total = repo.total_count().await?;
        tracing::debug!("[startup] rsi-digest: {total} events");
        Ok(Some(format!("{total} feedback events")))
    }
}
