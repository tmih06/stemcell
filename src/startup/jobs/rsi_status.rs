//! Startup job: report the RSI engine's boot snapshot.
//!
//! Report-only — this job does NOT run the template sync or any analysis
//! cycle. The persistent RSI engine ([`crate::brain::rsi::spawn_rsi_engine`])
//! still owns that work. Here we just inspect the on-disk sync state so the
//! startup-info line can show whether RSI will sync templates this boot.

use crate::startup::job::{StartupContext, StartupJob};
use async_trait::async_trait;

pub struct RsiStatusJob;

#[async_trait]
impl StartupJob for RsiStatusJob {
    fn name(&self) -> &'static str {
        "rsi-status"
    }

    async fn run(&self, _ctx: &StartupContext) -> anyhow::Result<Option<String>> {
        let state = crate::brain::rsi_sync::SyncState::load();
        let note = if crate::brain::rsi_sync::needs_sync(&state) {
            let from = if state.last_synced_version.is_empty() {
                "unsynced"
            } else {
                state.last_synced_version.as_str()
            };
            format!(
                "template sync pending ({} → v{})",
                from,
                crate::VERSION
            )
        } else {
            format!("templates up to date (v{})", crate::VERSION)
        };
        tracing::debug!("[startup] rsi-status: {note}");
        Ok(Some(note))
    }
}
