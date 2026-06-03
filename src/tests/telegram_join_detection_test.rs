//! Tests for Telegram bot join detection — captures member join events
//! before allowlist check and notifies owner with bot user ID.

use crate::channels::telegram::handler::format_bot_join_notification;

#[test]
fn format_bot_join_notification_contains_all_fields() {
    let notify = format_bot_join_notification("Test Group", -1001234567890, "atlas_bot", 8365623776);
    assert!(notify.contains("🤖 Bot joined"));
    assert!(notify.contains("Test Group"));
    assert!(notify.contains("-1001234567890"));
    assert!(notify.contains("@atlas_bot"));
    assert!(notify.contains("8365623776"));
    assert!(notify.contains("allowed_users"));
}

#[test]
fn format_bot_join_notification_handles_special_chars_in_title() {
    let notify = format_bot_join_notification("Crabs & Claws <Group>", -1234, "testbot", 9999);
    assert!(notify.contains("Crabs & Claws <Group>"));
    assert!(notify.contains("-1234"));
    assert!(notify.contains("@testbot"));
}

#[test]
fn format_bot_join_notification_preserves_numeric_precision() {
    let notify = format_bot_join_notification("G", 9999999999999, "bot", 1234567890123);
    assert!(notify.contains("9999999999999"));
    assert!(notify.contains("1234567890123"));
}

#[test]
fn format_bot_join_notification_actionable_instruction() {
    let notify = format_bot_join_notification("My Group", -5008492520, "opencrabsbot", 8478243969);
    assert!(notify.contains("Add this ID to allowed_users"));
    assert!(notify.contains("respond to it"));
}
