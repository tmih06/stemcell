//! Mission Control theme — colour and style constants.
//!
//! Pulled out so panel-specific renderers don't redefine colours
//! piecemeal, and so a future light/dark theme switch has one place to
//! change. Values mirror agentverse's mission control for visual
//! continuity across the two tools.
//!
//! Only the C8 skeleton's actual usages live here. C9 (inbox cards)
//! and C10 (activity/schedule badges) bring back the card and
//! status-colour constants alongside their first concrete callers, so
//! every name in this file has a live reader.

use ratatui::style::{Color, Modifier, Style};

// ── Panel chrome ────────────────────────────────────────────────────────────

/// Panel border when not focused.
pub const BORDER_IDLE: Color = Color::Rgb(50, 50, 70);
/// Panel border when focused — matches the panel's accent colour.
pub const BORDER_INBOX_FOCUS: Color = Color::Cyan;
pub const BORDER_ACTIVITY_FOCUS: Color = Color::Yellow;
pub const BORDER_SCHEDULE_FOCUS: Color = Color::Green;

/// Backdrop wash drawn behind every panel before content.
pub const BACKDROP: Color = Color::Rgb(18, 18, 24);

// ── Text ────────────────────────────────────────────────────────────────────

pub const TEXT_MUTED: Color = Color::Rgb(80, 80, 100);
pub const TEXT_DIM: Color = Color::Rgb(60, 60, 80);

pub const HELP_BAR: Color = Color::Rgb(100, 100, 120);

// ── Helpers ────────────────────────────────────────────────────────────────

pub fn title_style(accent: Color) -> Style {
    Style::default().fg(accent).add_modifier(Modifier::BOLD)
}

pub fn muted() -> Style {
    Style::default().fg(TEXT_MUTED)
}

pub fn dim() -> Style {
    Style::default().fg(TEXT_DIM)
}

pub fn help_bar_style() -> Style {
    Style::default().fg(HELP_BAR)
}
