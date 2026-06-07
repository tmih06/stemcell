//! Statusline dialog top-level renderer.
//!
//! Layout:
//!
//! ```text
//! ┌─ Status bar fields ───────────────────────────────┐
//! │                                                   │
//! │   [x] Session name                                │
//! │ ▸ [x] Provider / model                            │
//! │   [ ] Profile                                     │
//! │   [x] Working directory                           │
//! │   ...                                             │
//! │                                                   │
//! │ ↑↓/jk: move   Space: toggle   Esc: close          │
//! └───────────────────────────────────────────────────┘
//! ```

use crate::tui::app::App;
use crate::tui::app::statusline_dialog::FIELDS;
use crate::tui::render::palette;

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

/// Render the full statusline dialog into `area`.
pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // field list
            Constraint::Length(1), // help bar
        ])
        .split(area);

    draw_list(frame, app, chunks[0]);
    if chunks[1].height > 0 {
        draw_help_bar(frame, chunks[1]);
    }
}

fn draw_list(frame: &mut Frame, app: &App, area: Rect) {
    let selected = app.statusline_dialog.selected_index.min(FIELDS.len() - 1);
    let cfg = &app.statusline_fields;

    let mut lines: Vec<Line> = Vec::with_capacity(FIELDS.len() + 1);
    lines.push(Line::raw(""));

    for (idx, spec) in FIELDS.iter().enumerate() {
        let on = (spec.get)(cfg);
        let is_sel = idx == selected;

        let marker = if is_sel { " ▸ " } else { "   " };
        let checkbox = if on { "[x] " } else { "[ ] " };

        let marker_style = Style::default().fg(palette::TEAL);
        let checkbox_style = if on {
            Style::default().fg(palette::TEAL)
        } else {
            palette::muted()
        };
        let label_style = if is_sel {
            Style::default()
                .fg(palette::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(palette::TEXT_PRIMARY)
        };

        lines.push(Line::from(vec![
            Span::styled(marker, marker_style),
            Span::styled(checkbox, checkbox_style),
            Span::styled(spec.label, label_style),
        ]));
    }

    let block = Block::default()
        .title(" Status bar fields ")
        .title_style(palette::title_style(palette::TEAL))
        .borders(Borders::ALL)
        .border_set(symbols::border::ROUNDED)
        .border_style(Style::default().fg(palette::TEAL));

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_help_bar(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" ↑↓/jk", palette::dim()),
        Span::styled(": move   ", palette::dim()),
        Span::styled("Space", palette::dim()),
        Span::styled(": toggle   ", palette::dim()),
        Span::styled("Esc", palette::dim()),
        Span::styled(": close", palette::dim()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}
