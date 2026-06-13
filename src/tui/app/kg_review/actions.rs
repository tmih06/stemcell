//! Side-effecting actions for the `/kg` review screen.
//!
//! Thin layer over the `brain::kg::review` service: every git/queue operation
//! lives there, so a UI approve is byte-identical to one driven any other way.
//! These functions just load config, call the service, refresh the cached
//! snapshots, and surface a status-bar notification.

use crate::tui::app::App;
use crate::tui::app::kg_review::state::KgView;
use crate::tui::events::AppMode;

/// Max commits fetched for the Log view.
const LOG_LIMIT: usize = 50;

/// Enter the `/kg` review screen and load the pending queue. Optional
/// `subcommand` (from `/kg <sub>`) routes to a non-default view or action.
pub async fn dispatch(app: &mut App, subcommand: &str, _full_input: &str) {
    match subcommand {
        "" | "review" | "queue" => open(app).await,
        "log" => open_log(app).await,
        "revert" => {
            open(app).await;
            revert_last(app).await;
        }
        other => {
            app.mode = AppMode::KgReview;
            app.kg_review.reset();
            refresh(app).await;
            notify(
                app,
                &format!("Unknown /kg subcommand \"{other}\" — showing queue"),
            );
        }
    }
}

/// Enter the review screen on the Queue view.
pub async fn open(app: &mut App) {
    app.mode = AppMode::KgReview;
    app.kg_review.reset();
    app.kg_review.view = KgView::Queue;
    refresh(app).await;
}

/// Enter the review screen on the Log view.
pub async fn open_log(app: &mut App) {
    app.mode = AppMode::KgReview;
    app.kg_review.reset();
    app.kg_review.view = KgView::Log;
    refresh(app).await;
}

/// Re-fetch the active view's data into the cached snapshots, then refresh the
/// selected batch's diff.
pub async fn refresh(app: &mut App) {
    let Some(config) = load_config(app) else {
        return;
    };
    let pool = app.agent_service.context().pool();

    match app.kg_review.view {
        KgView::Queue => {
            match crate::brain::kg::review::list_pending(pool.clone()).await {
                Ok(batches) => app.kg_review.batches = batches,
                Err(e) => {
                    app.kg_review.batches = Vec::new();
                    notify(app, &format!("Failed to load queue: {e}"));
                }
            }
            clamp_selection(app);
            load_diff(app).await;
        }
        KgView::Log => {
            match crate::brain::kg::review::log(&config, LOG_LIMIT) {
                Ok(log) => app.kg_review.log = log,
                Err(e) => {
                    app.kg_review.log = Vec::new();
                    notify(app, &format!("Failed to load log: {e}"));
                }
            }
            clamp_selection(app);
        }
    }
}

/// Load the diff for the currently-selected batch into the right pane.
pub async fn load_diff(app: &mut App) {
    app.kg_review.diff_scroll = 0;
    let Some(config) = load_config(app) else {
        app.kg_review.diff = None;
        return;
    };
    let pool = app.agent_service.context().pool();
    let Some(batch) = app.kg_review.selected_batch().cloned() else {
        app.kg_review.diff = None;
        return;
    };
    match crate::brain::kg::review::batch_diff(&config, pool, &batch.id).await {
        Ok(diff) => app.kg_review.diff = Some(diff),
        Err(e) => app.kg_review.diff = Some(format!("(failed to load diff: {e})")),
    }
}

/// Approve the selected pending batch (merge into main).
pub async fn approve_selected(app: &mut App) {
    let Some(config) = load_config(app) else {
        return;
    };
    let pool = app.agent_service.context().pool();
    let Some(batch) = app.kg_review.selected_batch().cloned() else {
        return;
    };
    let msg = match crate::brain::kg::review::approve(&config, pool, &batch.id).await {
        Ok(crate::brain::kg::git_review::MergeOutcome::Merged(_)) => {
            format!("Approved \"{}\" — merged into vault", batch.summary)
        }
        Ok(crate::brain::kg::git_review::MergeOutcome::Conflicted(paths)) => {
            format!(
                "Conflict on {} — left queued for manual resolution ({} file(s))",
                batch.summary,
                paths.len()
            )
        }
        Err(e) => format!("Approve failed: {e}"),
    };
    notify(app, &msg);
    refresh(app).await;
}

/// Decline the selected pending batch (drop its branch).
pub async fn decline_selected(app: &mut App) {
    let Some(config) = load_config(app) else {
        return;
    };
    let pool = app.agent_service.context().pool();
    let Some(batch) = app.kg_review.selected_batch().cloned() else {
        return;
    };
    let msg = match crate::brain::kg::review::decline(&config, pool, &batch.id).await {
        Ok(()) => format!("Declined \"{}\" — dropped", batch.summary),
        Err(e) => format!("Decline failed: {e}"),
    };
    notify(app, &msg);
    refresh(app).await;
}

/// Revert the most-recently-approved batch.
pub async fn revert_last(app: &mut App) {
    let Some(config) = load_config(app) else {
        return;
    };
    let pool = app.agent_service.context().pool();
    let msg = match crate::brain::kg::review::revert_last(&config, pool).await {
        Ok(_) => "Reverted the last approved batch".to_string(),
        Err(e) => format!("Revert failed: {e}"),
    };
    notify(app, &msg);
    refresh(app).await;
}

/// Restore the vault to the selected log commit (destructive — armed by a prior
/// `confirm_restore`).
pub async fn restore_selected(app: &mut App) {
    let Some(config) = load_config(app) else {
        return;
    };
    let pool = app.agent_service.context().pool();
    let Some(commit) = app.kg_review.selected_commit().cloned() else {
        return;
    };
    let msg = match crate::brain::kg::review::restore(&config, pool, &commit.sha).await {
        Ok(()) => format!(
            "Restored vault to {}",
            &commit.sha[..commit.sha.len().min(8)]
        ),
        Err(e) => format!("Restore failed: {e}"),
    };
    app.kg_review.confirm_restore = false;
    notify(app, &msg);
    refresh(app).await;
}

fn clamp_selection(app: &mut App) {
    let count = app.kg_review.row_count();
    if count == 0 {
        app.kg_review.selected_index = 0;
    } else {
        app.kg_review.selected_index = app.kg_review.selected_index.min(count - 1);
    }
}

fn load_config(app: &mut App) -> Option<crate::config::Config> {
    match crate::config::Config::load() {
        Ok(c) => Some(c),
        Err(e) => {
            notify(app, &format!("Failed to load config: {e}"));
            None
        }
    }
}

fn notify(app: &mut App, message: &str) {
    app.set_notification(message, false);
}
