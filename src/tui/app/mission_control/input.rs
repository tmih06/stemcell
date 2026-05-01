//! Keyboard handling for `AppMode::MissionControl`.
//!
//! C8: only Esc is wired (closes MC, returns to Chat). Tab / j / k /
//! Enter / a / r land in C11 alongside the detail popup and inbox
//! actions. The handler returns `true` when it consumed the key, so
//! the dispatcher in `tui/app/input.rs` knows whether to fall through
//! to the default chat-mode handlers.

use crate::tui::app::App;
use crate::tui::events::AppMode;
use crossterm::event::{KeyCode, KeyEvent};

/// Handle a key press while Mission Control is the active mode.
/// Returns `true` if the key was consumed.
pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    // Detail popup, when open, swallows Esc first — second Esc closes MC.
    if app.mc.detail_open {
        if matches!(key.code, KeyCode::Esc) {
            app.mc.detail_open = false;
            return true;
        }
        // C11 will fan out the rest of the popup keys.
        return false;
    }

    match key.code {
        KeyCode::Esc => {
            close(app);
            true
        }
        _ => false,
    }
}

/// Close Mission Control and restore the previous app mode (Chat).
pub fn close(app: &mut App) {
    app.mode = AppMode::Chat;
    app.mc.detail_open = false;
    // Selection state is intentionally preserved across open/close
    // cycles in C8 — re-opening MC resumes where you left off. C11
    // adds a `reset_on_open` toggle if we want the other behaviour.
}
