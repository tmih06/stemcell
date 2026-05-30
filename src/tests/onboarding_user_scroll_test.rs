//! Tests for manual Page Up / Page Down scrolling in the onboarding
//! wizard.
//!
//! Without the manual scroll the form's last 1-3 fields fell off the
//! viewport on small or zoomed-in terminals because the focus-driven
//! scroll only moved when the user Tabbed to a field, not when they
//! just wanted to peek at one. This test covers the user-facing
//! contract on the wizard side; the rendering side reads
//! `wizard.user_scroll_offset` and adds it to the focus-driven
//! scroll each frame.

use crate::tui::onboarding::OnboardingWizard;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

#[test]
fn fresh_wizard_starts_with_zero_scroll() {
    let w = OnboardingWizard::default();
    assert_eq!(w.user_scroll_offset, 0);
}

#[test]
fn page_down_advances_scroll_by_five() {
    let mut w = OnboardingWizard::default();
    let _ = w.handle_key(key(KeyCode::PageDown));
    assert_eq!(w.user_scroll_offset, 5);
}

#[test]
fn multiple_page_downs_accumulate() {
    let mut w = OnboardingWizard::default();
    let _ = w.handle_key(key(KeyCode::PageDown));
    let _ = w.handle_key(key(KeyCode::PageDown));
    let _ = w.handle_key(key(KeyCode::PageDown));
    assert_eq!(w.user_scroll_offset, 15);
}

#[test]
fn page_up_subtracts_five() {
    let mut w = OnboardingWizard::default();
    let _ = w.handle_key(key(KeyCode::PageDown));
    let _ = w.handle_key(key(KeyCode::PageDown));
    assert_eq!(w.user_scroll_offset, 10);
    let _ = w.handle_key(key(KeyCode::PageUp));
    assert_eq!(w.user_scroll_offset, 5);
}

#[test]
fn page_up_at_zero_saturates_does_not_panic() {
    // Page Up when already at the top must not underflow. Without
    // saturating_sub the u16 would wrap to ~65k and the focus-driven
    // scroll would jump to the end.
    let mut w = OnboardingWizard::default();
    let _ = w.handle_key(key(KeyCode::PageUp));
    let _ = w.handle_key(key(KeyCode::PageUp));
    let _ = w.handle_key(key(KeyCode::PageUp));
    assert_eq!(w.user_scroll_offset, 0);
}

#[test]
fn next_step_resets_user_scroll() {
    // A Page Down on the Workspace step shouldn't leave the user
    // mid-scroll on ProviderAuth — each new screen should start at
    // the top so the title bar and progress dots are visible.
    let mut w = OnboardingWizard::default();
    let _ = w.handle_key(key(KeyCode::PageDown));
    let _ = w.handle_key(key(KeyCode::PageDown));
    assert_eq!(w.user_scroll_offset, 10);
    w.next_step();
    assert_eq!(w.user_scroll_offset, 0);
}

#[test]
fn prev_step_resets_user_scroll() {
    let mut w = OnboardingWizard::default();
    let _ = w.handle_key(key(KeyCode::PageDown));
    assert_eq!(w.user_scroll_offset, 5);
    let _ = w.prev_step();
    assert_eq!(w.user_scroll_offset, 0);
}

#[test]
fn page_keys_do_not_advance_step_or_focus() {
    // Page Up / Page Down are scroll-only. Tab still advances focus,
    // Enter still confirms. Confirm the scroll keys don't bleed into
    // either.
    let mut w = OnboardingWizard::default();
    let original_step = w.step;
    let original_field = w.focused_field;
    let _ = w.handle_key(key(KeyCode::PageDown));
    assert_eq!(w.step, original_step, "Page Down must not change the step");
    assert_eq!(
        w.focused_field, original_field,
        "Page Down must not move focus"
    );
}
