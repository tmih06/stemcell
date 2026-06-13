//! Side-effect actions for the `/export` dialog.

use crate::tui::app::App;
use crate::tui::events::AppMode;

/// Open the export dialog. Resets selection so re-opening always lands on the
/// top row.
pub fn open(app: &mut App) {
    if app.mode == AppMode::Export {
        return;
    }
    app.mode = AppMode::Export;
    app.export_dialog.reset();
}
