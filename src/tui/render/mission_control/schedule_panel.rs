//! Schedule panel — bottom right (60% × 50%), cron + pending approvals.
//!
//! C8 stub: empty bordered placeholder. Service + entries land in C10.

use super::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Paragraph};

pub fn draw(frame: &mut Frame, area: Rect, focused: bool) {
    let border = if focused {
        theme::BORDER_SCHEDULE_FOCUS
    } else {
        theme::BORDER_IDLE
    };
    let block = Block::default()
        .title(" Schedule ")
        .title_style(theme::title_style(theme::BORDER_SCHEDULE_FOCUS))
        .borders(Borders::ALL)
        .border_set(ratatui::symbols::border::ROUNDED)
        .border_style(Style::default().fg(border));
    let placeholder = Paragraph::new("\n  (Cron + pending approvals will land here in C10)")
        .style(theme::muted())
        .block(block);
    frame.render_widget(placeholder, area);
}
