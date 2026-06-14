//! Unit tests for the "jump to latest" toast counter.
//!
//! `count_new_messages_since_anchor` powers the toast shown when the user has
//! scrolled up: it counts the responses that arrived after the last message
//! they saw at the bottom (the `unread_anchor`). Contract:
//!   * count assistant replies with visible text and system notices,
//!   * skip tool-call groups, empty thinking-only turns, and history markers,
//!   * stay correct when older history is *prepended* (PageUp loads more),
//!   * return 0 with no anchor (pinned to bottom) or a stale/missing anchor.

use crate::tui::app::{
    DisplayMessage, ToolCallEntry, ToolCallGroup, count_new_messages_since_anchor,
};
use uuid::Uuid;

fn msg(role: &str, content: &str) -> DisplayMessage {
    DisplayMessage {
        id: Uuid::new_v4(),
        role: role.to_string(),
        content: content.to_string(),
        timestamp: chrono::Utc::now(),
        token_count: None,
        cost: None,
        approval: None,
        approve_menu: None,
        details: None,
        expanded: false,
        tool_group: None,
    }
}

/// Assistant message with no visible text (reasoning only) — must not count.
fn thinking_only() -> DisplayMessage {
    let mut m = msg("assistant", "   \n\t ");
    m.details = Some("thinking about the problem...".to_string());
    m
}

/// A finalized tool-call group message — must not count as a "new message".
fn tool_group() -> DisplayMessage {
    let mut m = msg("tool_group", "1 tool call");
    m.tool_group = Some(ToolCallGroup {
        calls: vec![ToolCallEntry {
            description: "Tool call".to_string(),
            success: true,
            details: None,
            completed: true,
            tool_input: serde_json::Value::Null,
        }],
        expanded: false,
    });
    m
}

#[test]
fn no_anchor_means_zero() {
    let messages = vec![msg("assistant", "hello")];
    assert_eq!(count_new_messages_since_anchor(&messages, None), 0);
}

#[test]
fn anchor_at_bottom_means_zero() {
    let messages = vec![msg("user", "hi"), msg("assistant", "hello")];
    let anchor = messages.last().unwrap().id;
    assert_eq!(count_new_messages_since_anchor(&messages, Some(anchor)), 0);
}

#[test]
fn counts_assistant_replies_after_anchor() {
    let anchor_msg = msg("assistant", "first reply");
    let anchor = anchor_msg.id;
    let messages = vec![
        msg("user", "hi"),
        anchor_msg,
        msg("assistant", "second reply"),
        msg("assistant", "third reply"),
    ];
    assert_eq!(count_new_messages_since_anchor(&messages, Some(anchor)), 2);
}

#[test]
fn counts_system_notices_but_skips_tool_groups_and_thinking() {
    let anchor_msg = msg("assistant", "reply");
    let anchor = anchor_msg.id;
    let messages = vec![
        anchor_msg,
        thinking_only(),                       // skipped: empty assistant turn
        tool_group(),                          // skipped: tool-call group
        msg("system", "Operation cancelled."), // counted
        msg("assistant", "final reply"),       // counted
    ];
    assert_eq!(count_new_messages_since_anchor(&messages, Some(anchor)), 2);
}

#[test]
fn prepended_history_does_not_inflate_count() {
    // Transcript at scroll-up time: the anchor plus one newer reply.
    let anchor_msg = msg("assistant", "anchored reply");
    let anchor = anchor_msg.id;
    let mut tail = vec![anchor_msg, msg("assistant", "new reply")];

    // Simulate PageUp prepending older history (a marker + an old exchange)
    // at the FRONT of the transcript, as `load_more_history` does.
    let mut messages = vec![
        msg("history_marker", "20 earlier messages"),
        msg("user", "old question"),
        msg("assistant", "old answer"),
    ];
    messages.append(&mut tail);

    // Only the single reply after the anchor counts: scanning from the back
    // stops at the anchor before reaching the prepended old answer.
    assert_eq!(count_new_messages_since_anchor(&messages, Some(anchor)), 1);
}

#[test]
fn missing_anchor_means_zero() {
    let messages = vec![msg("assistant", "a"), msg("assistant", "b")];
    // An id not present in the transcript (anchor message was cleared/reloaded).
    let stale = Uuid::new_v4();
    assert_eq!(count_new_messages_since_anchor(&messages, Some(stale)), 0);
}
