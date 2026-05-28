//! Rename-session tool.
//!
//! Lets the agent rename the current session so the title in `/sessions`
//! reflects what the conversation is actually about. Channels and the
//! TUI stamp a static label at session creation (e.g. "A2A: <first 60
//! chars>", "Telegram: <chat>"); this tool is the agent-callable path
//! to update that label once the conversation has enough context.

use super::error::Result;
use super::r#trait::{Tool, ToolCapability, ToolExecutionContext, ToolResult};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;

pub struct RenameSessionTool;

#[derive(Debug, Deserialize)]
struct RenameInput {
    /// New title for the session. Must be a non-empty, non-whitespace
    /// string. To revert to the channel-default title, the user has to
    /// recreate the session — `rename_session` no longer accepts an
    /// empty string (issue #128: empty rename silently wiped the title
    /// on Telegram and the session became unidentifiable in
    /// `/sessions`).
    title: String,
}

#[async_trait]
impl Tool for RenameSessionTool {
    fn name(&self) -> &str {
        "rename_session"
    }

    fn description(&self) -> &str {
        "Rename the current session. Use this once a session has enough context that the \
         channel-default title (e.g. 'A2A: <first 60 chars>', 'Telegram: <chat>') is no \
         longer descriptive. Provide a short, specific title (3-8 words) that reflects \
         the actual work being done. The title must be non-empty (whitespace-only is \
         rejected); to revert to no-title, the session has to be recreated."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "New session title (1-200 chars, non-whitespace).",
                    "minLength": 1,
                    "maxLength": 200
                }
            },
            "required": ["title"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        // Metadata-only update — no filesystem, no shell, no network.
        vec![]
    }

    fn requires_approval(&self) -> bool {
        // Low blast radius: title is a display string, easily reverted
        // via the same tool or via the TUI rename UI. Don't gate it
        // behind approval — the agent should use it proactively.
        false
    }

    async fn execute(&self, input: Value, context: &ToolExecutionContext) -> Result<ToolResult> {
        let parsed: RenameInput = serde_json::from_value(input)?;
        let trimmed = parsed.title.trim();
        if trimmed.is_empty() {
            // Issue #128: empty/whitespace-only title was silently
            // accepted and wiped the session's stored title, making
            // the row appear as "Untitled" in /sessions. Reject so
            // the model has to provide a real title and can't leave
            // sessions unlabeled.
            return Ok(ToolResult::error(
                "Title cannot be empty or whitespace-only. Provide a short, specific \
                 title (3-8 words) that reflects the actual work being done."
                    .into(),
            ));
        }
        if trimmed.len() > 200 {
            return Ok(ToolResult::error(
                "Title too long (max 200 chars). Pick something shorter and more specific.".into(),
            ));
        }

        let svc_ctx = match context.service_context.as_ref() {
            Some(c) => c,
            None => {
                return Ok(ToolResult::error(
                    "No service context available — rename_session can only run inside an \
                     active session loop."
                        .into(),
                ));
            }
        };

        let session_svc = crate::services::SessionService::new(svc_ctx.clone());
        let session_id = context.session_id;
        let new_title = trimmed.to_string();

        match session_svc
            .update_session_title(session_id, Some(new_title.clone()))
            .await
        {
            Ok(()) => Ok(ToolResult::success(format!(
                "Session renamed to '{new_title}'."
            ))),
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to rename session {}: {}",
                session_id, e
            ))),
        }
    }
}
