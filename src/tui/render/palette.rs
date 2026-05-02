//! Shared TUI palette + style helpers.
//!
//! Brand-level colours — orange, teal, white — used by every dialog
//! that wants visual continuity with the canonical OpenCrabs look in
//! `sessions.rs` and `usage/dashboard.rs`. Panel-specific aliases
//! (e.g. `BORDER_INBOX_FOCUS`) live in their owning module's local
//! theme file.

use ratatui::style::{Color, Modifier, Style};

// ── Brand palette ──────────────────────────────────────────────────────────

/// Crab orange — primary brand colour, used for titles and the
/// "active" / "warn" accent in panels that adopt it.
pub const ORANGE: Color = Color::Rgb(215, 100, 20);
/// Teal accent — primary action / "selected" colour.
pub const TEAL: Color = Color::Cyan;
/// Soft white — passive / informational accent.
pub const WHITE: Color = Color::Rgb(220, 220, 220);

// ── Text ────────────────────────────────────────────────────────────────────

pub const TEXT_PRIMARY: Color = Color::Rgb(200, 200, 210);
pub const TEXT_SECONDARY: Color = Color::Rgb(140, 140, 160);
pub const TEXT_MUTED: Color = Color::Rgb(80, 80, 100);
pub const TEXT_DIM: Color = Color::Rgb(60, 60, 80);

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
