//! Statusline dialog top-level renderer.
//!
//! Rendered as a popup overlay above the input box so the live status bar
//! stays visible underneath while the user toggles fields.

use crate::tui::app::App;
use crate::tui::app::statusline_dialog::FIELDS;
use crate::tui::render::{input::fit_dropdown, palette};

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

const HELP_TEXT: &str = "↑↓/jk move  Space toggle  Esc close";
const POPUP_CHROME: u16 = 3; // 2 border rows + 1 help row
const POPUP_MIN_WIDTH: u16 = 38;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OverlayLayout {
    area: Rect,
    scroll_offset: usize,
    visible_items: usize,
}

/// Render the statusline popup using `input_area` as the preferred anchor.
pub fn draw(frame: &mut Frame, app: &App, input_area: Rect, preview_area: Rect) {
    let Some(layout) = overlay_layout(
        app.statusline_dialog.selected_index,
        input_area,
        preview_area,
    ) else {
        return;
    };

    let block = Block::default()
        .title(" Status line ")
        .title_style(palette::title_style(palette::TEAL))
        .borders(Borders::ALL)
        .border_set(symbols::border::ROUNDED)
        .border_style(Style::default().fg(palette::TEAL));

    frame.render_widget(Clear, layout.area);
    let inner = block.inner(layout.area);
    frame.render_widget(block, layout.area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    draw_list(
        frame,
        app,
        chunks[0],
        layout.scroll_offset,
        layout.visible_items,
    );
    if chunks[1].height > 0 {
        draw_help_bar(frame, chunks[1]);
    }
}

fn overlay_layout(
    selected_index: usize,
    input_area: Rect,
    preview_area: Rect,
) -> Option<OverlayLayout> {
    let total_items = FIELDS.len() as u16;
    if total_items == 0 {
        return None;
    }

    let selected = selected_index.min(FIELDS.len().saturating_sub(1)) as u16;
    let anchored_max_width = input_area.width.saturating_sub(1).max(1);
    let rows_above_input = input_area.y.saturating_sub(preview_area.y);

    if let Some(fit) = fit_dropdown(total_items, selected, rows_above_input, POPUP_CHROME) {
        let width = overlay_width(anchored_max_width);
        let x = if input_area.width > width {
            input_area.x.saturating_add(1)
        } else {
            input_area.x
        };
        return Some(OverlayLayout {
            area: Rect {
                x,
                y: input_area.y.saturating_sub(fit.height),
                width,
                height: fit.height,
            },
            scroll_offset: fit.scroll_offset as usize,
            visible_items: fit.visible_items as usize,
        });
    }

    let fit = fit_dropdown(total_items, selected, preview_area.height, POPUP_CHROME)?;
    let width = overlay_width(preview_area.width.saturating_sub(2).max(1));
    Some(OverlayLayout {
        area: Rect {
            x: preview_area.x + preview_area.width.saturating_sub(width) / 2,
            y: preview_area.y + preview_area.height.saturating_sub(fit.height) / 2,
            width,
            height: fit.height,
        },
        scroll_offset: fit.scroll_offset as usize,
        visible_items: fit.visible_items as usize,
    })
}

fn overlay_width(max_width: u16) -> u16 {
    let max_label_width = FIELDS
        .iter()
        .map(|spec| spec.label.chars().count())
        .max()
        .unwrap_or(0) as u16;
    let list_row_width = 3u16.saturating_add(4).saturating_add(max_label_width);
    let help_width = HELP_TEXT.chars().count() as u16;
    let desired = list_row_width.max(help_width).saturating_add(2);
    desired.max(POPUP_MIN_WIDTH).min(max_width).max(1)
}

fn draw_list(frame: &mut Frame, app: &App, area: Rect, scroll_offset: usize, visible_items: usize) {
    let selected = app.statusline_dialog.selected_index.min(FIELDS.len() - 1);
    let cfg = &app.statusline_fields;

    let visible_end = (scroll_offset + visible_items).min(FIELDS.len());
    let mut lines: Vec<Line> = Vec::with_capacity(visible_end.saturating_sub(scroll_offset));
    for (idx, spec) in FIELDS
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_end.saturating_sub(scroll_offset))
    {
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

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_help_bar(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![Span::styled(HELP_TEXT, palette::dim())]);
    frame.render_widget(Paragraph::new(line), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u16, y: u16, width: u16, height: u16) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn anchors_overlay_above_input_when_room_exists() {
        let input_area = rect(0, 18, 80, 3);
        let preview_area = rect(0, 0, 80, 21);

        let layout = overlay_layout(0, input_area, preview_area).expect("overlay should render");

        assert_eq!(layout.area.y + layout.area.height, input_area.y);
        assert_eq!(layout.area.height, FIELDS.len() as u16 + POPUP_CHROME);
        assert_eq!(layout.scroll_offset, 0);
    }

    #[test]
    fn scrolls_visible_window_when_terminal_is_short() {
        let input_area = rect(0, 7, 80, 3);
        let preview_area = rect(0, 0, 80, 10);

        let layout = overlay_layout(FIELDS.len() - 1, input_area, preview_area)
            .expect("overlay should render");

        assert_eq!(layout.area.height, 7);
        assert_eq!(layout.visible_items, 4);
        assert!(
            (layout.scroll_offset..layout.scroll_offset + layout.visible_items)
                .contains(&(FIELDS.len() - 1)),
            "selected row must stay visible inside the clipped popup"
        );
    }

    #[test]
    fn falls_back_to_centered_overlay_when_no_rows_exist_above_input() {
        let input_area = rect(0, 2, 80, 3);
        let preview_area = rect(0, 0, 80, 12);

        let layout = overlay_layout(0, input_area, preview_area).expect("overlay should render");

        assert_ne!(
            layout.area.y + layout.area.height,
            input_area.y,
            "centered fallback should not stay pinned above the input"
        );
        assert!(layout.area.y >= preview_area.y);
        assert!(layout.area.y + layout.area.height <= preview_area.y + preview_area.height);
    }
}
