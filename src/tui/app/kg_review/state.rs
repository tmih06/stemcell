//! `/kg` review-screen app-side state.
//!
//! One struct so `AppState` carries a single `pub kg_review: KgReviewState`
//! field; input handling lives in `input.rs`, side-effecting git/queue calls in
//! `actions.rs`, rendering in `tui::render::kg_review`. Mirrors the Mission
//! Control state pattern.

use crate::brain::kg::git_review::LogEntry;
use crate::db::KgPendingBatch;

/// Which view the `/kg` screen is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KgView {
    /// The pending-batch review queue (the landing view).
    #[default]
    Queue,
    /// Vault commit history (`/kg log`).
    Log,
}

/// All `/kg`-screen runtime state.
#[derive(Debug, Clone, Default)]
pub struct KgReviewState {
    /// Which view keyboard input affects.
    pub view: KgView,
    /// Selected row index within the active view's list.
    pub selected_index: usize,
    /// Cached pending batches — populated by `actions::refresh`, read by the
    /// renderer. Pre-fetched so each frame is a borrow, not a DB read.
    pub batches: Vec<KgPendingBatch>,
    /// Cached commit log — populated when the Log view is opened.
    pub log: Vec<LogEntry>,
    /// Diff text for the selected batch, shown in the right pane. Refetched on
    /// selection change. `None` until first loaded.
    pub diff: Option<String>,
    /// Vertical scroll offset into the diff pane.
    pub diff_scroll: u16,
    /// Whether a destructive `restore` confirmation is pending for the selected
    /// log commit. `r` arms it, a second `r`/Enter confirms, Esc cancels.
    pub confirm_restore: bool,
}

impl KgReviewState {
    /// Reset view + selection when (re-)entering the `/kg` screen.
    pub fn reset(&mut self) {
        self.view = KgView::default();
        self.selected_index = 0;
        self.diff_scroll = 0;
        self.confirm_restore = false;
    }

    /// Number of rows in the active view.
    pub fn row_count(&self) -> usize {
        match self.view {
            KgView::Queue => self.batches.len(),
            KgView::Log => self.log.len(),
        }
    }

    /// The selected pending batch, if the Queue view is active and in range.
    pub fn selected_batch(&self) -> Option<&KgPendingBatch> {
        match self.view {
            KgView::Queue => self.batches.get(self.selected_index),
            KgView::Log => None,
        }
    }

    /// The selected log commit, if the Log view is active and in range.
    pub fn selected_commit(&self) -> Option<&LogEntry> {
        match self.view {
            KgView::Log => self.log.get(self.selected_index),
            KgView::Queue => None,
        }
    }
}
