//! Tests for the first-time onboarding welcome message.
//!
//! Covers the `is_first_time` flag on `OnboardingWizard` and the
//! `WELCOME_MESSAGE` constant used in `dialogs.rs`.

use crate::tui::onboarding::{OnboardingWizard, WELCOME_MESSAGE};

// ── OnboardingWizard::is_first_time ─────────────────────────────

#[test]
fn wizard_is_first_time_defaults_to_false() {
    let w = OnboardingWizard::new();
    assert!(!w.is_first_time);
}

#[test]
fn wizard_is_first_time_can_be_set_true() {
    let mut w = OnboardingWizard::new();
    w.is_first_time = true;
    assert!(w.is_first_time);
}

#[test]
fn wizard_is_first_time_can_be_toggled_back() {
    let mut w = OnboardingWizard::new();
    w.is_first_time = true;
    assert!(w.is_first_time);
    w.is_first_time = false;
    assert!(!w.is_first_time);
}

// ── WELCOME_MESSAGE constant ────────────────────────────────────

#[test]
fn welcome_message_is_non_empty() {
    assert!(!WELCOME_MESSAGE.is_empty());
}

#[test]
fn welcome_message_contains_cronjob() {
    assert!(
        WELCOME_MESSAGE.to_lowercase().contains("cronjob"),
        "welcome message should mention cronjob"
    );
}

#[test]
fn welcome_message_contains_heartbeat() {
    assert!(
        WELCOME_MESSAGE.to_lowercase().contains("heartbeat"),
        "welcome message should mention heartbeat"
    );
}

#[test]
fn welcome_message_contains_onboard_complete() {
    assert!(
        WELCOME_MESSAGE.to_lowercase().contains("onboard complete"),
        "welcome message should contain 'onboard complete'"
    );
}

#[test]
fn welcome_message_contains_brain_files() {
    assert!(
        WELCOME_MESSAGE.to_lowercase().contains("brain files"),
        "welcome message should mention brain files"
    );
}

#[test]
fn welcome_message_starts_with_expected_phrase() {
    assert!(
        WELCOME_MESSAGE.starts_with("Holy shit, we are live."),
        "welcome message should open with the expected phrase"
    );
}

#[test]
fn welcome_message_ends_with_question() {
    assert!(
        WELCOME_MESSAGE.ends_with('?'),
        "welcome message should end with a question mark"
    );
}
