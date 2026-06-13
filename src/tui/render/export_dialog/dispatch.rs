//! Export dialog top-level renderer.
//!
//! Rendered as a centered popup overlay above the chat shell: a short list of
//! export targets (Copy / Export to file / Both), each with a one-line
//! description under the highlighted row.

use crate::tui::app::App;
use crate::tui::app::export_dialog::EXPORT_OPTIONS;
use crate::tui::render::palette;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

const HELP_TEXT: &str = "↑↓/jk move  Enter select  Esc cancel";
const POPUP_WIDTH: u16 = 52;

/// Render the export popup, centered in `area`.
pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    if EXPORT_OPTIONS.is_empty() {
        return;
    }

    // One row per option + one description row + one help row, plus borders.
    let body_height = EXPORT_OPTIONS.len() as u16 + 2;
    let popup_height = body_height + 2;
    let width = POPUP_WIDTH.min(area.width.max(1));
    let height = popup_height.min(area.height.max(1));
    let popup = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };

    let block = Block::default()
        .title(" Export session ")
        .title_style(palette::title_style(palette::TEAL))
        .borders(Borders::ALL)
        .border_set(symbols::border::ROUNDED)
        .border_style(Style::default().fg(palette::TEAL));

    frame.render_widget(Clear, popup);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(EXPORT_OPTIONS.len() as u16),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    draw_list(frame, app, chunks[0]);
    if chunks[1].height > 0 {
        draw_description(frame, app, chunks[1]);
    }
    if chunks[2].height > 0 {
        let line = Line::from(vec![Span::styled(HELP_TEXT, palette::dim())]);
        frame.render_widget(Paragraph::new(line), chunks[2]);
    }
}

fn draw_list(frame: &mut Frame, app: &App, area: Rect) {
    let selected = app
        .export_dialog
        .selected_index
        .min(EXPORT_OPTIONS.len() - 1);

    let mut lines: Vec<Line> = Vec::with_capacity(EXPORT_OPTIONS.len());
    for (idx, opt) in EXPORT_OPTIONS.iter().enumerate() {
        let is_sel = idx == selected;
        let marker = if is_sel { " ▸ " } else { "   " };
        let marker_style = Style::default().fg(palette::TEAL);
        let label_style = if is_sel {
            Style::default()
                .fg(palette::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette::TEXT_PRIMARY)
        };
        lines.push(Line::from(vec![
            Span::styled(marker, marker_style),
            Span::styled(opt.label, label_style),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_description(frame: &mut Frame, app: &App, area: Rect) {
    let selected = app
        .export_dialog
        .selected_index
        .min(EXPORT_OPTIONS.len() - 1);
    let desc = EXPORT_OPTIONS[selected].description;
    let line = Line::from(vec![Span::styled(format!("   {desc}"), palette::muted())]);
    frame.render_widget(Paragraph::new(line), area);
}
