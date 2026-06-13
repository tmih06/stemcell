//! Tests for the `/export` slash command: the pure transcript builder and the
//! dialog's keystroke decision logic.
//!
//! Context (2026-06-13): `/export` renders the current session's full
//! transcript (user input, reasoning, tool calls with inputs/outputs, assistant
//! responses) and delivers it via clipboard, file, or both. These tests cover
//! the `App`-free `build_transcript` formatter plus the pure `decide` keystroke
//! contract, and guard that `/export` stays registered in `SLASH_COMMANDS`.

use crate::tui::app::SLASH_COMMANDS;
use crate::tui::app::export_dialog::input::{KeyOutcome, decide};
use crate::tui::app::export_dialog::state::{EXPORT_OPTIONS, ExportDialogState};
use crate::tui::app::messaging::{ExportHeader, build_transcript};
use crate::tui::app::{DisplayMessage, ToolCallEntry, ToolCallGroup};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn header() -> ExportHeader<'static> {
    ExportHeader {
        title: "My Session",
        id: uuid::Uuid::nil(),
        model: "claude-opus-4-8",
        provider: "anthropic",
        created_at: chrono::Utc::now(),
        token_count: 1234,
        cost: 0.05,
    }
}

fn msg(role: &str, content: &str) -> DisplayMessage {
    DisplayMessage {
        id: uuid::Uuid::new_v4(),
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

#[test]
fn transcript_contains_header_metadata() {
    let report = build_transcript(&header(), &[]);
    assert!(report.contains("# My Session"));
    assert!(report.contains("anthropic / claude-opus-4-8"));
    assert!(report.contains("**Messages:** 0"));
}

#[test]
fn transcript_renders_user_and_assistant_roles() {
    let messages = [msg("user", "hello there"), msg("assistant", "hi back")];
    let report = build_transcript(&header(), &messages);
    let u = report.find("👤 User").expect("user role labelled");
    let a = report
        .find("🤖 Assistant")
        .expect("assistant role labelled");
    assert!(u < a, "messages must render in order:\n{report}");
    assert!(report.contains("hello there"));
    assert!(report.contains("hi back"));
}

#[test]
fn transcript_renders_reasoning_from_details() {
    let mut m = msg("assistant", "the answer is 42");
    m.details = Some("first I considered the question".to_string());
    let report = build_transcript(&header(), &[m]);
    assert!(
        report.contains("_Reasoning:_"),
        "reasoning block must be labelled:\n{report}"
    );
    assert!(report.contains("first I considered the question"));
    assert!(report.contains("the answer is 42"));
}

#[test]
fn transcript_renders_tool_calls_with_input_and_output() {
    let mut m = msg("tool_group", "1 tool call");
    m.tool_group = Some(ToolCallGroup {
        expanded: false,
        calls: vec![ToolCallEntry {
            description: "read_file path=src/main.rs".to_string(),
            success: true,
            details: Some("fn main() {}".to_string()),
            completed: true,
            tool_input: serde_json::json!({ "path": "src/main.rs" }),
        }],
    });
    let report = build_transcript(&header(), &[m]);
    assert!(report.contains("🔧 Tool: read_file path=src/main.rs [ok]"));
    assert!(report.contains("_Input:_"));
    assert!(report.contains("\"path\": \"src/main.rs\""));
    assert!(report.contains("_Output:_"));
    assert!(report.contains("fn main() {}"));
}

#[test]
fn transcript_marks_failed_tool_calls() {
    let mut m = msg("tool_group", "1 tool call");
    m.tool_group = Some(ToolCallGroup {
        expanded: false,
        calls: vec![ToolCallEntry {
            description: "bash false".to_string(),
            success: false,
            details: None,
            completed: true,
            tool_input: serde_json::Value::Null,
        }],
    });
    let report = build_transcript(&header(), &[m]);
    assert!(
        report.contains("[FAILED]"),
        "failed tool calls must be flagged:\n{report}"
    );
}

#[test]
fn transcript_skips_history_marker() {
    let messages = [
        msg("history_marker", "12 earlier messages"),
        msg("user", "go"),
    ];
    let report = build_transcript(&header(), &messages);
    assert!(
        !report.contains("12 earlier messages"),
        "paging chrome must not leak into the transcript:\n{report}"
    );
}

#[test]
fn empty_transcript_renders_without_panic() {
    let report = build_transcript(&header(), &[]);
    assert!(report.contains("# My Session"));
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn esc_closes_without_exporting() {
    let mut s = ExportDialogState::default();
    assert_eq!(decide(&mut s, key(KeyCode::Esc)), KeyOutcome::Close);
}

#[test]
fn enter_confirms_selected() {
    let mut s = ExportDialogState { selected_index: 2 };
    assert_eq!(decide(&mut s, key(KeyCode::Enter)), KeyOutcome::Confirm(2));
    assert_eq!(s.selected_index, 2, "selection unchanged by confirm");
}

#[test]
fn navigation_wraps() {
    let mut s = ExportDialogState { selected_index: 0 };
    decide(&mut s, key(KeyCode::Up));
    assert_eq!(
        s.selected_index,
        EXPORT_OPTIONS.len() - 1,
        "up from top wraps to bottom"
    );
    decide(&mut s, key(KeyCode::Down));
    assert_eq!(s.selected_index, 0, "down from bottom wraps to top");
}

#[test]
fn export_command_registered() {
    assert!(
        SLASH_COMMANDS.iter().any(|c| c.name == "/export"),
        "/export must stay registered in SLASH_COMMANDS"
    );
}
