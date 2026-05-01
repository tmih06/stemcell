//! Mission Control theme — colour and style constants.
//!
//! Matches the canonical OpenCrabs palette used in `sessions.rs`,
//! `usage/dashboard.rs`, and `chat.rs`: orange + teal + white,
//! greys for neutrals, red reserved for destructive signals.
//!
//! The MC has no dark backdrop wash — it inherits the terminal
//! background, same as the Sessions and Help screens. Borders carry
//! all the visual structure.

use ratatui::style::{Color, Modifier, Style};

// ── Brand palette ──────────────────────────────────────────────────────────

/// Crab orange — primary brand colour, used for titles and the activity
/// panel's focus accent.
pub const ORANGE: Color = Color::Rgb(215, 100, 20);
/// Teal accent — primary action colour, used for the inbox panel's
/// focus, selected items, and command-kind badges.
pub const TEAL: Color = Color::Cyan;
/// Soft white — schedule panel's focus accent. Light enough to register
/// against a dark terminal background without yelling.
pub const WHITE: Color = Color::Rgb(220, 220, 220);

// ── Panel chrome ────────────────────────────────────────────────────────────

/// Panel border when not focused — neutral grey, same as `sessions.rs`.
pub const BORDER_IDLE: Color = Color::Rgb(120, 120, 120);
/// Per-panel focus accents.
pub const BORDER_INBOX_FOCUS: Color = TEAL;
pub const BORDER_ACTIVITY_FOCUS: Color = ORANGE;
pub const BORDER_SCHEDULE_FOCUS: Color = WHITE;

// ── Text ────────────────────────────────────────────────────────────────────

pub const TEXT_PRIMARY: Color = Color::Rgb(200, 200, 210);
pub const TEXT_SECONDARY: Color = Color::Rgb(140, 140, 160);
pub const TEXT_MUTED: Color = Color::Rgb(80, 80, 100);
pub const TEXT_DIM: Color = Color::Rgb(60, 60, 80);

pub const HELP_BAR: Color = Color::Rgb(120, 120, 120);

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
