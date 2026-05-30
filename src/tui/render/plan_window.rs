//! Visible-window math for the plan checklist widget.
//!
//! The plan widget needs to render a slice of the task list that fits
//! into the height the parent layout gave it. For plans that fit we
//! render every task. For overflowing plans we reserve one row each
//! for "… (N above)" / "… (M below)" indicators and center the
//! remaining slots on the currently-focused task so the user always
//! sees what is being worked on next.
//!
//! Kept in its own module — separate from `plan_widget.rs` — so the
//! pure index/slice math is unit-testable without spinning up a
//! `ratatui::Frame` or an `App`.

use crate::tui::plan::{PlanTask, TaskStatus};

/// Visible window into the task list — which slice to render and what
/// the on-screen offsets are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VisibleWindow {
    /// Index of the first task to render (0-based, inclusive).
    pub start: usize,
    /// Number of tasks rendered (inclusive of `start`).
    pub len: usize,
}

/// Index of the task that should anchor the visible window. Picks the
/// first task in `InProgress` state, falls back to the first
/// non-terminal task (Pending / Blocked / Failed), and falls back to
/// the last task when everything is already completed/skipped so the
/// user sees the tail of the run instead of the head.
pub(crate) fn current_task_index(tasks: &[PlanTask]) -> usize {
    if let Some(i) = tasks
        .iter()
        .position(|t| matches!(t.status, TaskStatus::InProgress))
    {
        return i;
    }
    if let Some(i) = tasks.iter().position(|t| {
        matches!(
            t.status,
            TaskStatus::Pending | TaskStatus::Blocked(_) | TaskStatus::Failed
        )
    }) {
        return i;
    }
    tasks.len().saturating_sub(1)
}

/// Pick the slice of the task list to render given the available row
/// budget and the index of the currently-focused task. When every task
/// fits we return the whole list. Otherwise we reserve one row for an
/// "… (N above)" indicator (when `start > 0`) and one for "… (M below)"
/// (when the window ends before the last task), and center the
/// remaining slots on `anchor`.
pub(crate) fn pick_visible_window(
    total: usize,
    available_rows: usize,
    anchor: usize,
) -> VisibleWindow {
    if total == 0 || available_rows == 0 {
        return VisibleWindow { start: 0, len: 0 };
    }
    if total <= available_rows {
        return VisibleWindow {
            start: 0,
            len: total,
        };
    }
    // Overflowing. Plan for both indicators worst-case (one row each
    // for "above" and "below"), then reclaim those rows if we end up
    // not needing the indicator on a given edge.
    let task_slots = available_rows.saturating_sub(2).max(1);
    let half = task_slots / 2;
    let mut start = anchor.saturating_sub(half);
    let max_start = total.saturating_sub(task_slots);
    if start > max_start {
        start = max_start;
    }
    let mut len = task_slots;
    if start == 0 {
        // No "above" indicator needed; reclaim its row.
        len = (len + 1).min(total);
    }
    if start + len == total && start > 0 {
        // No "below" indicator needed; reclaim its row by extending
        // the window backward (we can't grow forward — we're already
        // at the end of the list). Keep anchor visible.
        start = start.saturating_sub(1);
        len += 1;
        if anchor < start {
            start = anchor;
        }
    }
    VisibleWindow { start, len }
}
