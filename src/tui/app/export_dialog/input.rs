//! Keyboard handling for `AppMode::Export`.
//!
//! Dialog model: a short vertical list of export targets (Copy / Export to
//! file / Both). ↑/↓/j/k move the selection (wrapping); Enter confirms the
//! highlighted option; Esc closes without exporting.
//!
//! `decide` is pure (mutates only `ExportDialogState`, returns a `KeyOutcome`)
//! so the keystroke contract is unit-testable without an `App`. The App-level
//! `handle_key` applies the side effect (run the export, then close).

use super::state::{EXPORT_OPTIONS, ExportDialogState};
use crate::tui::app::App;
use crate::tui::events::AppMode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Effect of a keystroke the App wrapper has to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Key consumed; no App-level action required.
    Consumed,
    /// User wants to leave the dialog — switch back to Chat without exporting.
    Close,
    /// User confirmed the option at this index into `EXPORT_OPTIONS` — caller
    /// runs the export, then closes the dialog.
    Confirm(usize),
    /// Key not recognised.
    NotConsumed,
}

/// Top-level handler called from the App's keystroke dispatcher.
pub async fn handle_key(app: &mut App, key: KeyEvent) {
    match decide(&mut app.export_dialog, key) {
        KeyOutcome::Consumed | KeyOutcome::NotConsumed => {}
        KeyOutcome::Close => {
            app.mode = AppMode::Chat;
        }
        KeyOutcome::Confirm(idx) => {
            let target = EXPORT_OPTIONS.get(idx).map(|o| o.target);
            app.mode = AppMode::Chat;
            if let Some(target) = target {
                app.run_export(target).await;
            }
        }
    }
}

/// Pure decision function. Mutates `state`; returns the App-level effect.
pub fn decide(state: &mut ExportDialogState, key: KeyEvent) -> KeyOutcome {
    match key.code {
        KeyCode::Esc => KeyOutcome::Close,

        KeyCode::Enter | KeyCode::Char(' ') => KeyOutcome::Confirm(state.selected_index),

        KeyCode::Down | KeyCode::Tab => {
            move_selection(state, 1);
            KeyOutcome::Consumed
        }
        KeyCode::Up | KeyCode::BackTab => {
            move_selection(state, -1);
            KeyOutcome::Consumed
        }
        KeyCode::Char('j') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            move_selection(state, 1);
            KeyOutcome::Consumed
        }
        KeyCode::Char('k') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            move_selection(state, -1);
            KeyOutcome::Consumed
        }

        _ => KeyOutcome::NotConsumed,
    }
}

fn move_selection(state: &mut ExportDialogState, delta: i32) {
    let count = EXPORT_OPTIONS.len() as i32;
    if count == 0 {
        state.selected_index = 0;
        return;
    }
    // Positive-modulo idiom (Rust `%` is remainder, not Euclidean modulo).
    let cur = (state.selected_index as i32).min(count - 1);
    let next = ((cur + delta) % count + count) % count;
    state.selected_index = next as usize;
}
