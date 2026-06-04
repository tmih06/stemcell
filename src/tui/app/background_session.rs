//! Per-session live-state cache for non-focused panes.
//!
//! ## Problem
//!
//! Before this module existed, `AppState` held the live state for
//! **one session at a time** — `streaming_response`,
//! `streaming_reasoning`, `active_tool_group`, `is_processing`,
//! `processing_started_at`, `streaming_output_tokens`,
//! `last_input_tokens`, `display_token_count`, `tps_tracker`. Twenty
//! `TuiEvent` handlers in `state.rs` were gated on
//! `is_current_session(session_id)` and silently dropped events for
//! non-focused sessions. The split-pane renderer then showed an
//! inactive pane frozen at whatever was last loaded from the DB —
//! no live thinking, no in-progress tool calls, no streaming chunks
//! until the user tabbed back and `preload_pane_session` re-read
//! from disk. The inactive pane in the 2026-06-04 screenshot showed
//! gray "Thinking" and "X tool calls" markers from prior turns but
//! nothing about the active turn taking place in that session.
//!
//! ## Shape of the fix
//!
//! `BackgroundSessionState` is a sidecar struct that carries the
//! exact same live fields the focused `AppState` holds, but only
//! for sessions that aren't currently shown in the focused pane.
//! Event handlers route every incoming `TuiEvent` to either
//! `AppState` (focused) or
//! `AppState.background_sessions[session_id]` (background) via the
//! `SessionStateMut` enum returned by `App::session_state_mut`.
//!
//! On focus switch, `App::demote_to_background(old_sid)` saves the
//! current `AppState`'s live fields into a fresh
//! `BackgroundSessionState`, and `App::promote_to_foreground(
//! new_sid)` pops the new pane's entry into `AppState`. A
//! `ResponseComplete` arriving for a background session finalizes
//! its `streaming_response` into a `DisplayMessage` in
//! `pane_message_cache` and drops the background entry — so when
//! the user eventually focuses that pane, the finalized message is
//! already there waiting for them.
//!
//! ## What lives here vs. in DB
//!
//! - DB-persisted (`MessageRepository`): every committed message,
//!   token/cost stats, plan documents, session metadata. These get
//!   re-read on focus switch via `preload_pane_session`.
//! - `pane_message_cache` (already in `AppState`): a snapshot of
//!   `DisplayMessage`s for the inactive pane, refreshed whenever
//!   focus leaves a pane. Read-only.
//! - **This module**: the *in-flight turn* state that hasn't yet
//!   landed in either the DB or `pane_message_cache` — streaming
//!   chunks, the current tool-call group, the thinking buffer, live
//!   token counters. Without it the inactive pane misses every
//!   chunk between turns.

use crate::tui::app::state::{StreamingTpsTracker, ToolCallGroup};
use std::time::Instant;

/// Sidecar live-state cache for a non-focused session.
///
/// Mirrors the live-turn fields on `App` so an inactive pane can be
/// rendered with up-to-date thinking/tool/streaming state and
/// promoted back to foreground without losing accumulated work.
/// Fields are intentionally kept narrow to the *in-flight* turn —
/// finalized messages live in `pane_message_cache` or the DB.
#[derive(Debug, Default, Clone)]
pub struct BackgroundSessionState {
    /// Streaming response buffer for the in-progress turn. `None`
    /// between turns. Drained into a finalized `DisplayMessage` on
    /// `ResponseComplete` before this entry is dropped.
    pub streaming_response: Option<String>,
    /// Streaming reasoning ("Thinking …") buffer for the in-
    /// progress turn. Cleared at the start of every visible response
    /// chunk (same shape as `App.streaming_reasoning`).
    pub streaming_reasoning: Option<String>,
    /// Tool calls executing this turn. The renderer collapses these
    /// into the gray "X tool calls" bullet on the inactive pane.
    pub active_tool_group: Option<ToolCallGroup>,
    /// Whether the agent is mid-turn for this session. Drives the
    /// "[processing...]" status label on the inactive pane border.
    pub is_processing: bool,
    /// Instant the current turn started — used to render elapsed
    /// time and tok/s on the inactive pane footer when the user
    /// switches focus.
    pub processing_started_at: Option<Instant>,
    /// Last input-token count seen for this session (drives the
    /// `ctx: NK/200K` indicator on the inactive pane footer).
    pub last_input_tokens: Option<u32>,
    /// Running output-token count for the in-progress turn.
    pub streaming_output_tokens: u32,
    /// Current ctx-token display for the inactive pane.
    pub display_token_count: usize,
    /// Live tok/s tracker for this session — accumulates active
    /// streaming windows the same way the foreground tracker does
    /// so the rate stays accurate across focus switches. See
    /// `StreamingTpsTracker` doc for the active-window invariant.
    pub tps_tracker: StreamingTpsTracker,
}

impl BackgroundSessionState {
    /// True when there is *any* live state worth rendering. Empty
    /// background entries are dropped at cleanup time to keep the
    /// map bounded.
    pub fn has_live_state(&self) -> bool {
        self.streaming_response.is_some()
            || self.streaming_reasoning.is_some()
            || self.active_tool_group.is_some()
            || self.is_processing
            || self.streaming_output_tokens > 0
    }
}

/// Routing handle for an event handler.
///
/// Returned by `App::session_state_mut(session_id)`. The variant
/// carried tells the handler where to write — foreground (the
/// focused `App` fields) or background (a sidecar entry). Both
/// variants expose the same set of mutator methods below so the
/// 20 event-handler call sites stay one-liners regardless of
/// which session they're targeting.
///
/// Lifetime is tied to the borrow on `App`, so no clone is
/// performed and the handler can mutate either path in place.
pub enum SessionStateMut<'a> {
    Foreground(&'a mut crate::tui::app::App),
    Background(&'a mut BackgroundSessionState),
}

impl SessionStateMut<'_> {
    /// Append a streaming-response chunk. Foreground: also nudges
    /// the auto-scroll flag and invalidates the render cache.
    /// Background: just appends; render happens on focus switch.
    pub fn append_streaming_chunk(&mut self, text: &str) {
        match self {
            Self::Foreground(app) => app.append_streaming_chunk(text.to_string()),
            Self::Background(bg) => {
                bg.streaming_reasoning = None;
                bg.streaming_response
                    .get_or_insert_with(String::new)
                    .push_str(text);
            }
        }
    }

    /// Append a reasoning ("Thinking …") chunk. Both variants skip
    /// empty / whitespace-only chunks so the renderer never shows
    /// a "Thinking" label with no content.
    pub fn append_reasoning_chunk(&mut self, text: &str) {
        let is_whitespace = text.trim().is_empty();
        if text.is_empty() {
            return;
        }
        match self {
            Self::Foreground(app) => {
                if let Some(ref mut existing) = app.streaming_reasoning {
                    existing.push_str(text);
                } else if !is_whitespace {
                    app.streaming_reasoning = Some(text.to_string());
                }
                if app.auto_scroll {
                    app.scroll_offset = 0;
                }
            }
            Self::Background(bg) => {
                if let Some(ref mut existing) = bg.streaming_reasoning {
                    existing.push_str(text);
                } else if !is_whitespace {
                    bg.streaming_reasoning = Some(text.to_string());
                }
            }
        }
    }

    /// Mark the session as actively processing a turn.
    pub fn set_processing(&mut self, processing: bool) {
        match self {
            Self::Foreground(app) => {
                app.is_processing = processing;
                if processing && app.processing_started_at.is_none() {
                    app.processing_started_at = Some(Instant::now());
                } else if !processing {
                    app.processing_started_at = None;
                }
            }
            Self::Background(bg) => {
                bg.is_processing = processing;
                if processing && bg.processing_started_at.is_none() {
                    bg.processing_started_at = Some(Instant::now());
                } else if !processing {
                    bg.processing_started_at = None;
                }
            }
        }
    }

    /// Increment the streaming-output token counter and feed the
    /// tps tracker.
    pub fn add_streaming_output_tokens(&mut self, tokens: u32) {
        match self {
            Self::Foreground(app) => {
                app.streaming_output_tokens += tokens;
                app.advance_streaming_window();
            }
            Self::Background(bg) => {
                bg.streaming_output_tokens += tokens;
                bg.tps_tracker.advance(Instant::now());
            }
        }
    }

    /// Replace the active tool-call group. Used when a new batch of
    /// tool calls starts inside the same turn.
    pub fn set_active_tool_group(&mut self, group: Option<ToolCallGroup>) {
        match self {
            Self::Foreground(app) => app.active_tool_group = group,
            Self::Background(bg) => bg.active_tool_group = group,
        }
    }

    /// Borrow the active tool group mutably so the caller can flip
    /// individual entries from "executing" → "completed" without
    /// rebuilding the whole group.
    pub fn active_tool_group_mut(&mut self) -> Option<&mut ToolCallGroup> {
        match self {
            Self::Foreground(app) => app.active_tool_group.as_mut(),
            Self::Background(bg) => bg.active_tool_group.as_mut(),
        }
    }

    /// Update the display-token counter (drives the footer
    /// `ctx: NK/200K` indicator).
    pub fn set_display_token_count(&mut self, count: usize) {
        match self {
            Self::Foreground(app) => app.display_token_count = count,
            Self::Background(bg) => bg.display_token_count = count,
        }
    }

    /// Record the last-input-token count for the footer.
    pub fn set_last_input_tokens(&mut self, count: u32) {
        match self {
            Self::Foreground(app) => app.last_input_tokens = Some(count),
            Self::Background(bg) => bg.last_input_tokens = Some(count),
        }
    }

    /// Clear all in-flight turn state — used at `ResponseComplete`
    /// when the turn has been finalized into the message store.
    pub fn clear_turn_state(&mut self) {
        match self {
            Self::Foreground(app) => {
                app.streaming_response = None;
                app.streaming_reasoning = None;
                app.active_tool_group = None;
                app.is_processing = false;
                app.processing_started_at = None;
                app.streaming_output_tokens = 0;
            }
            Self::Background(bg) => {
                bg.streaming_response = None;
                bg.streaming_reasoning = None;
                bg.active_tool_group = None;
                bg.is_processing = false;
                bg.processing_started_at = None;
                bg.streaming_output_tokens = 0;
            }
        }
    }
}
