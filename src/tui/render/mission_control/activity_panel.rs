//! Activity panel — top right (60% × 50%), RSI activity feed.
//!
//! C8 stub: empty bordered placeholder. Service + entries land in C10.

use super::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Paragraph};

pub fn draw(frame: &mut Frame, area: Rect, focused: bool) {
    let border = if focused {
        theme::BORDER_ACTIVITY_FOCUS
    } else {
        theme::BORDER_IDLE
    };
    let block = Block::default()
        .title(" Activity ")
        .title_style(theme::title_style(theme::BORDER_ACTIVITY_FOCUS))
        .borders(Borders::ALL)
        .border_set(ratatui::symbols::border::ROUNDED)
        .border_style(Style::default().fg(border));
    let placeholder = Paragraph::new("\n  (RSI activity feed will land here in C10)")
        .style(theme::muted())
        .block(block);
    frame.render_widget(placeholder, area);
}
