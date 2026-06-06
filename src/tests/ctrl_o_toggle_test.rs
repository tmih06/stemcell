use crate::tui::app::input::ctrl_o_toggle_target;
use crate::tui::app::{DisplayMessage, ToolCallEntry, ToolCallGroup};
use uuid::Uuid;

fn reasoning_message(expanded: bool, details: &str) -> DisplayMessage {
    DisplayMessage {
        id: Uuid::new_v4(),
        role: "assistant".to_string(),
        content: String::new(),
        timestamp: chrono::Utc::now(),
        token_count: None,
        cost: None,
        approval: None,
        approve_menu: None,
        details: Some(details.to_string()),
        expanded,
        tool_group: None,
    }
}

fn tool_group_message(expanded: bool) -> DisplayMessage {
    DisplayMessage {
        id: Uuid::new_v4(),
        role: "tool_group".to_string(),
        content: "1 tool call".to_string(),
        timestamp: chrono::Utc::now(),
        token_count: None,
        cost: None,
        approval: None,
        approve_menu: None,
        details: None,
        expanded: false,
        tool_group: Some(ToolCallGroup {
            calls: vec![ToolCallEntry {
                description: "Tool call".to_string(),
                success: true,
                details: None,
                completed: true,
                tool_input: serde_json::Value::Null,
            }],
            expanded,
        }),
    }
}

#[test]
fn ctrl_o_reasoning_only_message_can_collapse() {
    let messages = vec![reasoning_message(true, "Reasoning details")];

    assert!(
        !ctrl_o_toggle_target(&messages, None),
        "expanded reasoning-only messages must collapse on Ctrl+O"
    );
}

#[test]
fn ctrl_o_uses_latest_reasoning_over_older_tool_group() {
    let messages = vec![
        tool_group_message(false),
        reasoning_message(true, "Latest expandable item"),
    ];

    assert!(
        !ctrl_o_toggle_target(&messages, None),
        "the latest expandable item should drive the next toggle target"
    );
}

#[test]
fn ctrl_o_ignores_blank_reasoning_details() {
    let messages = vec![reasoning_message(true, "   \n\t  ")];

    assert!(
        ctrl_o_toggle_target(&messages, None),
        "blank reasoning details should not force Ctrl+O into collapse mode"
    );
}
