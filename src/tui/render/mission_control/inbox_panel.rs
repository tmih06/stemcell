//! Inbox panel — left 40%, RSI proposals as cards.
//!
//! The card-rendering and selection logic lands in C9 alongside the
//! `inbox_service`. This file is the C8 stub: a bordered placeholder
//! that proves the layout slot is correctly sized and that the focus
//! border colour swaps when the panel is selected.

use super::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Paragraph};

pub fn draw(frame: &mut Frame, area: Rect, focused: bool) {
    let border = if focused {
        theme::BORDER_INBOX_FOCUS
    } else {
        theme::BORDER_IDLE
    };
    let block = Block::default()
        .title(" Inbox ")
        .title_style(theme::title_style(theme::BORDER_INBOX_FOCUS))
        .borders(Borders::ALL)
        .border_set(ratatui::symbols::border::ROUNDED)
        .border_style(Style::default().fg(border));
    let placeholder = Paragraph::new("\n  (RSI proposals will land here in C9)")
        .style(theme::muted())
        .block(block);
    frame.render_widget(placeholder, area);
}
