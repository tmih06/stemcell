//! Shared types for the Mission Control data layer.
//!
//! Each panel renders a uniform list of items. The type wrapping each
//! item carries enough metadata for the renderer to badge / colour
//! consistently across the three sources (inbox / activity / schedule)
//! without leaking the underlying storage shape.

use chrono::{DateTime, Utc};

/// One actionable item in the inbox panel — typically an RSI proposal.
#[derive(Debug, Clone)]
pub struct McInboxItem {
    /// Stable id for action-by-id flows (apply/reject). For RSI proposals
    /// this is `prop_tool_<uuid>` or `prop_cmd_<uuid>` from the inbox file.
    pub id: String,
    /// Short human label — slug name, e.g. "deploy_staging" or "/release".
    pub label: String,
    /// One-line summary surfaced under the label. The agent's rationale
    /// for why it proposed this, or a tool's command preview.
    pub summary: String,
    /// What kind of artifact this represents — drives the badge colour.
    pub kind: McInboxKind,
    /// Origin of the proposal (the `proposed_by` field on the inbox row,
    /// e.g. "rsi-autonomous"). Used for the "proposed by …" caption.
    pub source: String,
    /// When this item entered the inbox.
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McInboxKind {
    /// RSI-proposed dynamic tool (lands in `tools.toml` on apply).
    ProposedTool,
    /// RSI-proposed slash command (lands in `commands.toml` on apply).
    ProposedCommand,
}

impl McInboxKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ProposedTool => "tool",
            Self::ProposedCommand => "command",
        }
    }
}

/// One entry in the activity feed — RSI-emitted events worth surfacing.
#[derive(Debug, Clone)]
pub struct McActivity {
    pub timestamp: DateTime<Utc>,
    /// One-line summary, already truncated to a reasonable display length
    /// by the service layer.
    pub detail: String,
    /// Severity hint for colour selection in the renderer.
    pub level: McActivityLevel,
    /// Origin tag — "rsi", "compaction", "template-sync", etc. Stored as
    /// a free string so adding a new source doesn't require a migration.
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McActivityLevel {
    Info,
    Success,
    Warn,
    Error,
}

/// One scheduled / pending-action row.
#[derive(Debug, Clone)]
pub struct McScheduleItem {
    pub id: String,
    pub label: String,
    /// Free-text describing when/how it triggers — "0 9 * * *", "pending
    /// approval", "next at 14:00", etc.
    pub schedule: String,
    pub kind: McScheduleKind,
    /// `true` when the item is actively waiting on the user (e.g. a
    /// pending tool approval or a paused cron). Renders highlighted.
    pub awaiting_user: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McScheduleKind {
    /// Recurring cron job from `~/.opencrabs/cron/*.toml`.
    Cron,
    /// One-shot agent action waiting on a user approval prompt.
    PendingApproval,
}

impl McScheduleKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Cron => "cron",
            Self::PendingApproval => "approval",
        }
    }
}
