//! Side-effect actions invoked from Mission Control.
//!
//! C8: only `open` is wired — flips the app into MC mode. Apply / reject
//! / detail-open / detail-close land in C11.

use crate::tui::app::App;
use crate::tui::events::AppMode;

/// Enter Mission Control mode. Called by the `/mission-control` slash
/// command. Idempotent — re-opening from MC is a no-op.
pub fn open(app: &mut App) {
    if app.mode == AppMode::MissionControl {
        return;
    }
    app.mode = AppMode::MissionControl;
    // First open of a fresh session lands on the Inbox panel; subsequent
    // re-opens preserve focus state so the user resumes where they left
    // off. The `selected_index` is held by `McState`; we don't reset it
    // here for that reason.
}
