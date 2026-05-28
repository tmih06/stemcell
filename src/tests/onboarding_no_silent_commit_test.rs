//! Pin the "/onboard:provider Tab/Down must NOT commit" policy from the
//! 2026-05-28 bug report: user typed Enter on API key to fetch models,
//! never confirmed on the Model field, and woke up with their active
//! provider silently switched because Tab on Model called next_step()
//! which set quick_jump_done which triggered apply_config on the next
//! event tick.
//!
//! The fix: only Enter on the LAST step (Model for built-in providers,
//! CustomContextWindow for custom) sets quick_jump_done. Tab and Down
//! are no-ops on those fields. These tests fail if anyone reintroduces
//! the auto-commit path.

use crate::tui::onboarding::{AuthField, OnboardingStep, OnboardingWizard, WizardAction};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn quick_jump_wizard_on_model_step() -> OnboardingWizard {
    let mut w = OnboardingWizard::default();
    w.quick_jump = true;
    w.step = OnboardingStep::ProviderAuth;
    w.auth_field = AuthField::Model;
    // Pre-populate a tiny model list so `selected_model` is valid.
    w.ps.models = vec!["foo".to_string(), "bar".to_string()];
    w.ps.selected_model = 0;
    w
}

fn quick_jump_wizard_on_custom_context_window() -> OnboardingWizard {
    let mut w = OnboardingWizard::default();
    w.quick_jump = true;
    w.step = OnboardingStep::ProviderAuth;
    w.auth_field = AuthField::CustomContextWindow;
    w.ps.custom_name = "dialagram".to_string();
    w.ps.custom_model = "qwen-3.7-max-thinking".to_string();
    w.ps.context_window = "200000".to_string();
    w
}

// ─── Model field (built-in providers) ─────────────────────────────────

#[test]
fn enter_on_model_field_commits_quick_jump() {
    let mut w = quick_jump_wizard_on_model_step();
    let action = w.handle_key(key(KeyCode::Enter));
    assert_eq!(
        action,
        WizardAction::QuickJumpDone,
        "Enter on Model is the user's explicit confirmation — must commit"
    );
}

#[test]
fn tab_on_model_field_does_not_commit() {
    let mut w = quick_jump_wizard_on_model_step();
    let action = w.handle_key(key(KeyCode::Tab));
    assert!(
        !matches!(action, WizardAction::QuickJumpDone),
        "Tab on Model must NOT trigger commit — only Enter is the confirmation. \
         Pre-fix: Tab called next_step() which auto-saved (2026-05-28 bug)"
    );
    assert!(
        !w.quick_jump_done,
        "quick_jump_done must stay false after Tab"
    );
}

#[test]
fn down_on_model_field_does_not_commit() {
    let mut w = quick_jump_wizard_on_model_step();
    let action = w.handle_key(key(KeyCode::Down));
    assert!(
        !matches!(action, WizardAction::QuickJumpDone),
        "Down on Model must navigate the model list, never commit"
    );
    assert!(!w.quick_jump_done);
}

#[test]
fn backtab_on_model_field_does_not_commit() {
    let mut w = quick_jump_wizard_on_model_step();
    let action = w.handle_key(key(KeyCode::BackTab));
    assert!(
        !matches!(action, WizardAction::QuickJumpDone),
        "BackTab on Model goes back to ApiKey, must not commit"
    );
    assert!(!w.quick_jump_done);
}

#[test]
fn escape_on_model_field_in_quick_jump_cancels_not_commits() {
    let mut w = quick_jump_wizard_on_model_step();
    let action = w.handle_key(key(KeyCode::Esc));
    assert_eq!(
        action,
        WizardAction::Cancel,
        "Escape in quick_jump must always cancel cleanly, never commit"
    );
    assert!(!w.quick_jump_done);
}

// ─── CustomContextWindow (custom providers, last step) ────────────────

#[test]
fn enter_on_custom_context_window_commits() {
    let mut w = quick_jump_wizard_on_custom_context_window();
    let action = w.handle_key(key(KeyCode::Enter));
    assert_eq!(
        action,
        WizardAction::QuickJumpDone,
        "Enter on CustomContextWindow (last step of custom flow) is the explicit confirmation"
    );
}

#[test]
fn tab_on_custom_context_window_does_not_commit() {
    let mut w = quick_jump_wizard_on_custom_context_window();
    let action = w.handle_key(key(KeyCode::Tab));
    assert!(
        !matches!(action, WizardAction::QuickJumpDone),
        "Tab on CustomContextWindow must NOT commit (pre-fix it did)"
    );
    assert!(!w.quick_jump_done);
}

#[test]
fn down_on_custom_context_window_does_not_commit() {
    let mut w = quick_jump_wizard_on_custom_context_window();
    let action = w.handle_key(key(KeyCode::Down));
    assert!(
        !matches!(action, WizardAction::QuickJumpDone),
        "Down on CustomContextWindow must NOT commit (pre-fix it did)"
    );
    assert!(!w.quick_jump_done);
}
