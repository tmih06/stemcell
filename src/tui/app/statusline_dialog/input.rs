//! Keyboard handling for `AppMode::StatusLine`.
//!
//! Dialog model: a vertical checklist of status-bar fields. ↑/↓/j/k move
//! the selection (wrapping); Space / Enter toggle the highlighted field
//! in place; Esc closes. Toggles persist immediately to config.toml.
//!
//! `decide` is pure (mutates only `StatusLineDialogState`, returns a
//! `KeyOutcome`) so the keystroke contract is unit-testable without an
//! `App`. The App-level `handle_key` applies the side effects (flip the
//! live flag + write config).

use super::state::{FIELDS, StatusLineDialogState};
use crate::tui::app::App;
use crate::tui::events::AppMode;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Effect of a keystroke the App wrapper has to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyOutcome {
    /// Key consumed; no App-level action required.
    Consumed,
    /// User wants to leave the dialog — switch back to Chat.
    Close,
    /// User toggled the field at this index into `FIELDS` — caller flips
    /// the live flag and persists it.
    Toggle(usize),
    /// Key not recognised.
    NotConsumed,
}

/// Top-level handler called from the App's keystroke dispatcher.
pub async fn handle_key(app: &mut App, key: KeyEvent) {
    match decide(&mut app.statusline_dialog, key) {
        KeyOutcome::Consumed | KeyOutcome::NotConsumed => {}
        KeyOutcome::Close => {
            app.mode = AppMode::Chat;
        }
        KeyOutcome::Toggle(idx) => {
            let Some(spec) = FIELDS.get(idx) else { return };
            let new_val = !(spec.get)(&app.statusline_fields);
            (spec.set)(&mut app.statusline_fields, new_val);
            if let Err(e) = crate::config::Config::write_key(
                "statusline",
                spec.key,
                if new_val { "true" } else { "false" },
            ) {
                tracing::warn!("Failed to persist statusline.{}: {}", spec.key, e);
            }
        }
    }
}

/// Pure decision function. Mutates `state`; returns the App-level effect.
pub fn decide(state: &mut StatusLineDialogState, key: KeyEvent) -> KeyOutcome {
    match key.code {
        KeyCode::Esc => KeyOutcome::Close,

        KeyCode::Char(' ') | KeyCode::Enter => KeyOutcome::Toggle(state.selected_index),

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

fn move_selection(state: &mut StatusLineDialogState, delta: i32) {
    let count = FIELDS.len() as i32;
    if count == 0 {
        state.selected_index = 0;
        return;
    }
    // Positive-modulo idiom (Rust `%` is remainder, not Euclidean modulo).
    let cur = (state.selected_index as i32).min(count - 1);
    let next = ((cur + delta) % count + count) % count;
    state.selected_index = next as usize;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn esc_closes() {
        let mut s = StatusLineDialogState::default();
        assert_eq!(decide(&mut s, key(KeyCode::Esc)), KeyOutcome::Close);
    }

    #[test]
    fn space_toggles_selected() {
        let mut s = StatusLineDialogState { selected_index: 2 };
        assert_eq!(
            decide(&mut s, key(KeyCode::Char(' '))),
            KeyOutcome::Toggle(2)
        );
        // Selection unchanged by toggling.
        assert_eq!(s.selected_index, 2);
    }

    #[test]
    fn enter_toggles_selected() {
        let mut s = StatusLineDialogState { selected_index: 0 };
        assert_eq!(decide(&mut s, key(KeyCode::Enter)), KeyOutcome::Toggle(0));
    }

    #[test]
    fn down_wraps_around() {
        let mut s = StatusLineDialogState {
            selected_index: FIELDS.len() - 1,
        };
        decide(&mut s, key(KeyCode::Down));
        assert_eq!(s.selected_index, 0);
    }

    #[test]
    fn up_wraps_around() {
        let mut s = StatusLineDialogState { selected_index: 0 };
        decide(&mut s, key(KeyCode::Up));
        assert_eq!(s.selected_index, FIELDS.len() - 1);
    }

    #[test]
    fn jk_navigation() {
        let mut s = StatusLineDialogState { selected_index: 0 };
        decide(&mut s, key(KeyCode::Char('j')));
        assert_eq!(s.selected_index, 1);
        decide(&mut s, key(KeyCode::Char('k')));
        assert_eq!(s.selected_index, 0);
    }
}
