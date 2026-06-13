//! Startup job: report how many tools were registered and which ones.
//!
//! Read-only — the tool list is captured from the live registry at spawn time
//! (`StartupContext::tools`) so this reflects exactly what the agent is
//! equipped with, not what's merely compiled in. Folds the count into the
//! startup-info line and lists the tool names in the expandable details.

use crate::startup::job::{StartupContext, StartupJob};
use async_trait::async_trait;

pub struct ToolsLoadedJob;

#[async_trait]
impl StartupJob for ToolsLoadedJob {
    fn name(&self) -> &'static str {
        "tools-loaded"
    }

    async fn run(&self, ctx: &StartupContext) -> anyhow::Result<Option<String>> {
        let Some(tools) = ctx.tools.as_ref() else {
            return Ok(Some("skipped (no registry)".to_string()));
        };

        if tools.is_empty() {
            tracing::debug!("[startup] tools-loaded: 0 tools (chatbot mode?)");
            return Ok(Some("0 tools loaded (chatbot mode)".to_string()));
        }

        let mut sorted = tools.clone();
        sorted.sort();
        let count = sorted.len();
        tracing::debug!("[startup] tools-loaded: {count} tools");
        Ok(Some(format!("{count} tools: {}", sorted.join(", "))))
    }
}
