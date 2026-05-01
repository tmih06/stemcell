//! Mission Control top-level dispatch — paints the backdrop, computes
//! the panel layout, calls each panel renderer, then overlays the help
//! bar (and detail popup, when open).

use super::layout::{self, McLayout};
use super::theme;
use super::{activity_panel, detail_popup, inbox_panel, schedule_panel};

use crate::tui::app::App;
use crate::tui::app::mission_control::McPanel;

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Render Mission Control over the full content area `area`.
pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    paint_backdrop(frame, area);

    let McLayout {
        inbox,
        activity,
        schedule,
        help_bar,
    } = layout::compute(area);

    let focus = app.mc.focused_panel;
    inbox_panel::draw(frame, inbox, focus == McPanel::Inbox);
    activity_panel::draw(frame, activity, focus == McPanel::Activity);
    schedule_panel::draw(frame, schedule, focus == McPanel::Schedule);

    if help_bar.height > 0 {
        draw_help_bar(frame, help_bar);
    }

    if app.mc.detail_open {
        detail_popup::draw(frame, area);
    }
}

/// Wash the entire MC area in the dark backdrop colour. The panels then
/// paint over this with their own borders + content.
fn paint_backdrop(frame: &mut Frame, area: Rect) {
    let buf = frame.buffer_mut();
    for y in area.y..area.y.saturating_add(area.height) {
        for x in area.x..area.x.saturating_add(area.width) {
            if let Some(cell) = buf.cell_mut(Position::new(x, y)) {
                cell.set_char(' ');
                cell.set_bg(theme::BACKDROP);
            }
        }
    }
}

fn draw_help_bar(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" Tab", theme::help_bar_style()),
        Span::styled(": switch panel  ", theme::dim()),
        Span::styled("↑↓", theme::help_bar_style()),
        Span::styled(": navigate  ", theme::dim()),
        Span::styled("Enter", theme::help_bar_style()),
        Span::styled(": detail  ", theme::dim()),
        Span::styled("a", theme::help_bar_style()),
        Span::styled(": apply  ", theme::dim()),
        Span::styled("r", theme::help_bar_style()),
        Span::styled(": reject  ", theme::dim()),
        Span::styled("Esc", theme::help_bar_style()),
        Span::styled(": close", theme::dim()),
    ]);
    let bar = Paragraph::new(line).style(Style::default().bg(Color::Rgb(12, 12, 16)));
    frame.render_widget(bar, area);
}
