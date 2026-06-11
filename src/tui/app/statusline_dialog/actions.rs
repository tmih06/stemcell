//! Side-effect actions for the `/statusline` dialog.

use crate::tui::app::App;
use crate::tui::events::AppMode;

/// Open the statusline dialog. Resets selection so re-opening always lands
/// on the top row.
pub fn open(app: &mut App) {
    if app.mode == AppMode::StatusLine {
        return;
    }
    app.mode = AppMode::StatusLine;
    app.statusline_dialog.reset();
}
