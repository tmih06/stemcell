//! Inbox data service — pulls pending RSI proposals into a uniform
//! list of `McInboxItem` rows for the inbox panel.
//!
//! Stateless wrapper around `ProposalsStore`. Re-reads on every call so
//! the panel reflects what's actually on disk (the RSI background loop
//! and the user-facing agent can both touch the inbox concurrently —
//! see `rsi_proposals` module docs).

use super::types::{McInboxItem, McInboxKind};
use crate::brain::rsi_proposals::{CommandProposal, ProposalsStore, ToolProposal};

/// Read every pending tool + command proposal, sorted newest-first.
pub fn list() -> Vec<McInboxItem> {
    list_with_store(&ProposalsStore::new())
}

/// Same as [`list`] but takes an explicit store — used by tests so
/// they can point at a tmpdir without touching `~/.opencrabs/`.
pub fn list_with_store(store: &ProposalsStore) -> Vec<McInboxItem> {
    let mut items: Vec<McInboxItem> = store
        .list_tool_proposals()
        .into_iter()
        .map(item_from_tool)
        .chain(
            store
                .list_command_proposals()
                .into_iter()
                .map(item_from_command),
        )
        .collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.created_at));
    items
}

fn item_from_tool(p: ToolProposal) -> McInboxItem {
    let summary = match (&p.def.command, &p.def.url) {
        (Some(cmd), _) => format!("shell: {cmd}"),
        (_, Some(url)) => format!("{} {}", p.def.method.as_deref().unwrap_or("GET"), url),
        _ => "(no command / url)".to_string(),
    };
    McInboxItem {
        id: p.id,
        label: p.def.name,
        summary,
        kind: McInboxKind::ProposedTool,
        source: p.proposer,
        created_at: p.created_at,
    }
}

fn item_from_command(p: CommandProposal) -> McInboxItem {
    // Strip `prompt`/`system` action label out of the summary; the kind
    // badge already says "command", and the prompt body is the useful
    // bit a user wants to glance at.
    let summary = if p.command.prompt.is_empty() {
        format!("({})", p.command.action)
    } else {
        p.command.prompt.clone()
    };
    McInboxItem {
        id: p.id,
        label: p.command.name,
        summary,
        kind: McInboxKind::ProposedCommand,
        source: p.proposer,
        created_at: p.created_at,
    }
}
