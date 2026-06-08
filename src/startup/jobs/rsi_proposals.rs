//! Startup job: report pending RSI proposals awaiting review.
//!
//! Report-only — a synchronous on-disk count of proposals the autonomous RSI
//! loop has filed across its inboxes. Surfaced as its own line in the
//! collapsible startup-info report, with a pointer to where to review them.

use crate::startup::job::{StartupContext, StartupJob};
use async_trait::async_trait;

pub struct RsiProposalsJob;

#[async_trait]
impl StartupJob for RsiProposalsJob {
    fn name(&self) -> &'static str {
        "rsi-proposals"
    }

    async fn run(&self, _ctx: &StartupContext) -> anyhow::Result<Option<String>> {
        let pending = crate::brain::rsi_proposals::ProposalsStore::new().pending_count();
        let note = if pending > 0 {
            format!("{pending} proposal(s) pending review (/mission-control → Inbox)")
        } else {
            "no proposals pending".to_string()
        };
        tracing::debug!("[startup] rsi-proposals: {note}");
        Ok(Some(note))
    }
}
