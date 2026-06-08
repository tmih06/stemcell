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
        let sync_note = if crate::brain::rsi_sync::needs_sync(&state) {
            let from = if state.last_synced_version.is_empty() {
                "unsynced"
            } else {
                state.last_synced_version.as_str()
            };
            format!("template sync pending ({} → v{})", from, crate::VERSION)
        } else {
            format!("templates up to date (v{})", crate::VERSION)
        };

        // Pending RSI proposals waiting in the Mission Control inbox — a
        // synchronous on-disk count, so it belongs here at boot rather than as
        // a separate TUI banner. Kept actionable with a pointer to where to
        // review them.
        let pending = crate::brain::rsi_proposals::ProposalsStore::new().pending_count();
        let note = if pending > 0 {
            format!(
                "{sync_note}; {pending} proposal(s) pending review (/mission-control → Inbox)"
            )
        } else {
            sync_note
        };

        tracing::debug!("[startup] rsi-status: {note}");
        Ok(Some(note))
    }
}
