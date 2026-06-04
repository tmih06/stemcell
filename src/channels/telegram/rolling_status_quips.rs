//! Project-author-original rolling status quips for the Telegram
//! pre-tool reasoning phase.
//!
//! When the agent has been thinking for a while without yet calling a
//! tool, the status line falls back to one of these. They rotate every
//! ~15s so a 3-minute reasoning phase doesn't show the same message
//! for the entire wait (the bug observed 2026-06-03: "Marathon mode —
//! still on: <preview>" stuck for 3+ minutes because the previous
//! `pre_tool_rolling` only had four elapsed-time buckets and ran out
//! of variation past 60s).
//!
//! The list is the exact set introduced in commit `f5b5de1a`
//! (2026-05, "feat: rolling status quips on Telegram during long tool
//! execution") and consistent across every commit that touched it
//! through `512bf002` where it was removed. Restored verbatim.

/// The rolling-status quip pool, verbatim from `f5b5de1a`. Order
/// matters — `rotating_quip` indexes into this slice with
/// `(elapsed_secs / WINDOW_SECS) % len`, so reordering the entries
/// changes which one users see first.
pub(crate) const TOOL_STATUS_QUIPS: &[&str] = &[
    "☕ Grab a coffee — my sub-agents are on fire right now",
    "🦀 My crabs are working their claws off — hang tight",
    "🔥 Still cooking... deep in the code",
    "⚡ Sub-agents going brrr — almost there",
    "🧠 Thinking hard so you don't have to",
    "🏗️ Building something beautiful — one sec",
    "🎯 Locked in — the crabs are laser-focused",
    "🚀 Full speed ahead — engines at max",
    "💪 Crunching through the code like a boss",
    "🌊 Riding the wave — results incoming",
    "🎪 The circus is in town — all crabs performing",
    "🔧 Wrenching away at it — precision work",
    "🏎️ Pedal to the metal — no brakes",
    "🧪 Experimenting... for science!",
    "🎵 Working to the rhythm — almost done",
];

/// Seconds between quip rotations. Picked to be slow enough that a
/// user reading the line has time to register the words (less than
/// 5s would feel jittery), fast enough that a multi-minute marathon
/// cycles through several entries. With 15 quips × 15s = full pool
/// every 3m 45s.
const WINDOW_SECS: u64 = 15;

/// Pick the quip for the current elapsed time. `elapsed_secs` is the
/// total time since the user's message was received. Same input
/// always yields the same output — deterministic so the picker
/// stays stable across the multiple build-status ticks that happen
/// inside one rotation window.
pub(crate) fn rotating_quip(elapsed_secs: u64) -> &'static str {
    let idx = (elapsed_secs / WINDOW_SECS) as usize % TOOL_STATUS_QUIPS.len();
    TOOL_STATUS_QUIPS[idx]
}
