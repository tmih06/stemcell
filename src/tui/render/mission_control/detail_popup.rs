//! Detail popup overlay — opens on Enter, dismissed on Esc.
//!
//! C8 stub: skeleton renderer that just clears a centred area. The real
//! per-panel detail content (proposal body, activity event, schedule
//! row) lands alongside C11's keyboard navigation.

use super::theme;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

/// Render a centred popup occupying ~60% × 70% of `area`.
pub fn draw(frame: &mut Frame, area: Rect) {
    let pw = (area.width * 60 / 100).max(30);
    let ph = (area.height * 70 / 100).max(10);
    let px = area.x + area.width.saturating_sub(pw) / 2;
    let py = area.y + area.height.saturating_sub(ph) / 2;
    let popup = Rect::new(px, py, pw, ph);

    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Detail ")
        .title_style(theme::title_style(theme::BORDER_INBOX_FOCUS))
        .borders(Borders::ALL)
        .border_set(ratatui::symbols::border::ROUNDED)
        .border_style(Style::default().fg(theme::BORDER_INBOX_FOCUS));
    let body = Paragraph::new("\n  (Detail body lands in C11)")
        .style(theme::muted())
        .block(block);
    frame.render_widget(body, popup);
}
