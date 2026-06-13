//! Keyboard handling for `AppMode::KgReview`.
//!
//! A full-screen overlay (like Mission Control): the App's match-arm doesn't
//! fall through to chat handlers, so unrecognised keys are simply ignored. The
//! decision logic is a pure `decide` over `&mut KgReviewState` returning a
//! `KeyOutcome`; the async `handle_key` wrapper routes outcomes that need
//! git/queue side-effects to `actions`.

use super::state::{KgReviewState, KgView};
use crate::tui::app::App;
use crate::tui::events::AppMode;
use crossterm::event::{KeyCode, KeyEvent};

/// App-level effect of a keystroke on the `/kg` screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Key consumed; no App-level action required.
    Consumed,
    /// Leave the `/kg` screen, back to Chat.
    Close,
    /// Selection moved — reload the right-pane diff (Queue view only).
    SelectionChanged,
    /// Approve the selected pending batch.
    Approve,
    /// Decline the selected pending batch.
    Decline,
    /// Restore the vault to the selected log commit (already confirmed).
    Restore,
}

/// Top-level handler from the App keystroke dispatcher.
pub async fn handle_key(app: &mut App, key: KeyEvent) {
    match decide(&mut app.kg_review, key) {
        KeyOutcome::Consumed => {}
        KeyOutcome::Close => app.mode = AppMode::Chat,
        KeyOutcome::SelectionChanged => super::actions::load_diff(app).await,
        KeyOutcome::Approve => super::actions::approve_selected(app).await,
        KeyOutcome::Decline => super::actions::decline_selected(app).await,
        KeyOutcome::Restore => super::actions::restore_selected(app).await,
    }
}

/// Pure decision function — mutates `state`, returns the App-level effect.
pub fn decide(state: &mut KgReviewState, key: KeyEvent) -> KeyOutcome {
    let count = state.row_count();

    // A pending restore confirmation captures the next keystroke: a second `r`
    // (or Enter) commits, anything else cancels — so a destructive reset never
    // fires on a single keypress.
    if state.confirm_restore {
        return match key.code {
            KeyCode::Char('r') | KeyCode::Enter => KeyOutcome::Restore,
            _ => {
                state.confirm_restore = false;
                KeyOutcome::Consumed
            }
        };
    }

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => KeyOutcome::Close,
        // Tab toggles between the Queue and Log views.
        KeyCode::Tab => {
            state.view = match state.view {
                KgView::Queue => KgView::Log,
                KgView::Log => KgView::Queue,
            };
            state.selected_index = 0;
            state.diff_scroll = 0;
            KeyOutcome::SelectionChanged
        }
        KeyCode::Up | KeyCode::Char('k') => {
            move_selection(state, count, -1);
            KeyOutcome::SelectionChanged
        }
        KeyCode::Down | KeyCode::Char('j') => {
            move_selection(state, count, 1);
            KeyOutcome::SelectionChanged
        }
        // Scroll the diff pane (Queue view) with PageUp/PageDown.
        KeyCode::PageUp => {
            state.diff_scroll = state.diff_scroll.saturating_sub(10);
            KeyOutcome::Consumed
        }
        KeyCode::PageDown => {
            state.diff_scroll = state.diff_scroll.saturating_add(10);
            KeyOutcome::Consumed
        }
        // Approve / decline act on the Queue view only.
        KeyCode::Char('a') => {
            if state.view == KgView::Queue && count > 0 {
                KeyOutcome::Approve
            } else {
                KeyOutcome::Consumed
            }
        }
        KeyCode::Char('d') => {
            if state.view == KgView::Queue && count > 0 {
                KeyOutcome::Decline
            } else {
                KeyOutcome::Consumed
            }
        }
        // `r` means revert-pending in the Log view (arms confirmation) — a
        // destructive vault reset, so it needs the two-key confirm.
        KeyCode::Char('r') => {
            if state.view == KgView::Log && count > 0 {
                state.confirm_restore = true;
            }
            KeyOutcome::Consumed
        }
        _ => KeyOutcome::Consumed,
    }
}

fn move_selection(state: &mut KgReviewState, count: usize, delta: i32) {
    if count == 0 {
        state.selected_index = 0;
        return;
    }
    let max_idx = count - 1;
    let cur = state.selected_index.min(max_idx) as i32;
    state.selected_index = (cur + delta).clamp(0, max_idx as i32) as usize;
    state.diff_scroll = 0;
}
