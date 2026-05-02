//! Side-effect actions invoked from Mission Control.
//!
//! `open` flips the app into MC mode and pre-fetches the data each
//! panel needs (activity entries from disk, schedule rows from the
//! cron-jobs DB) so the synchronous render path never has to hit the
//! filesystem or run a SQL query mid-frame.

use crate::brain::mission_control::{activity_service, schedule_service};
use crate::tui::app::App;
use crate::tui::events::AppMode;

/// Maximum activity feed entries cached for the panel. The journal
/// itself is unbounded, but the panel only ever displays a window —
/// reading more wastes memory without changing what the user sees.
const ACTIVITY_LIMIT: usize = 100;

/// Enter Mission Control mode. Idempotent — re-opening from MC is a
/// no-op (the snapshots stay as they were; call `refresh` explicitly
/// if a re-fetch is required).
pub async fn open(app: &mut App) {
    if app.mode == AppMode::MissionControl {
        return;
    }
    app.mode = AppMode::MissionControl;
    refresh(app).await;
}

/// Re-fetch every panel's data into the cached snapshots in `McState`.
/// Called on `open` and (later, in C11) on a refresh keystroke.
pub async fn refresh(app: &mut App) {
    app.mc.activity = activity_service::recent(ACTIVITY_LIMIT);
    let pool = app.agent_service.context().pool();
    app.mc.schedule = schedule_service::list(pool).await;
}
