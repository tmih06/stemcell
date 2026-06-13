//! `/kg` review-screen rendering.
//!
//! A two-pane full-screen overlay: the left pane is the active list (pending
//! batch queue or vault commit log), the right pane shows the selected batch's
//! diff (Queue view) or is hidden (Log view). Pairs with
//! `tui::app::kg_review` (state/input/actions) and the `brain::kg::review`
//! service. Pure read of `app.kg_review` snapshots — no service calls here.

use crate::tui::app::App;
use crate::tui::app::kg_review::state::KgView;
use crate::tui::render::palette::{ORANGE, TEAL, TEXT_DIM, TEXT_PRIMARY, TEXT_SECONDARY, WHITE};

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

/// Neutral list-row border / idle accent (matches the Sessions list).
const BORDER_IDLE: Color = Color::Rgb(120, 120, 120);
/// Diff line colours.
const DIFF_ADD: Color = Color::Rgb(120, 200, 120);
const DIFF_DEL: Color = Color::Rgb(220, 120, 120);
const DIFF_HUNK: Color = Color::Rgb(120, 160, 220);

/// Top-level `/kg` screen entry, called from the render dispatch.
pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    // Reserve a one-row help bar at the bottom.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);
    let body = rows[0];
    let help_area = rows[1];

    match app.kg_review.view {
        KgView::Queue => draw_queue(frame, app, body),
        KgView::Log => draw_log(frame, app, body),
    }

    draw_help_bar(frame, app, help_area);
}

// ── Queue view ────────────────────────────────────────────────────────────

fn draw_queue(frame: &mut Frame, app: &App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);
    draw_batch_list(frame, app, cols[0]);
    draw_diff_pane(frame, app, cols[1]);
}

fn draw_batch_list(frame: &mut Frame, app: &App, area: Rect) {
    let batches = &app.kg_review.batches;
    let title = format!(" Review queue ({}) ", batches.len());
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(TEAL).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_set(symbols::border::ROUNDED)
        .border_style(Style::default().fg(TEAL));

    if batches.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "\n  Nothing pending. The agent's memory writes will appear here for review.",
            Style::default().fg(TEXT_DIM),
        )))
        .wrap(Wrap { trim: false })
        .block(block);
        frame.render_widget(empty, area);
        return;
    }

    let selected = app.kg_review.selected_index.min(batches.len() - 1);
    let mut lines: Vec<Line> = Vec::new();
    for (idx, batch) in batches.iter().enumerate() {
        let is_sel = idx == selected;
        let marker = if is_sel { "❯ " } else { "  " };
        let name_style = if is_sel {
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_PRIMARY)
        };
        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(TEAL)),
            Span::styled(truncate(&batch.summary, 48), name_style),
        ]));
        // Stat line: +ins / -del across N files, plus a conflicted flag.
        let mut spans = vec![Span::styled(
            format!("    {} file(s)  ", batch.files_changed),
            Style::default().fg(TEXT_DIM),
        )];
        spans.push(Span::styled(
            format!("+{}", batch.insertions),
            Style::default().fg(DIFF_ADD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("-{}", batch.deletions),
            Style::default().fg(DIFF_DEL),
        ));
        if batch.status == "conflicted" {
            spans.push(Span::styled(
                "  ⚠ conflict",
                Style::default().fg(ORANGE).add_modifier(Modifier::BOLD),
            ));
        }
        lines.push(Line::from(spans));
        lines.push(Line::raw(""));
    }

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, area);
}

fn draw_diff_pane(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Diff ")
        .title_style(Style::default().fg(ORANGE).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_set(symbols::border::ROUNDED)
        .border_style(Style::default().fg(BORDER_IDLE));

    let lines: Vec<Line> = match &app.kg_review.diff {
        Some(diff) if !diff.trim().is_empty() => diff.lines().map(diff_line).collect(),
        Some(_) => vec![Line::from(Span::styled(
            "(no changes)",
            Style::default().fg(TEXT_DIM),
        ))],
        None => vec![Line::from(Span::styled(
            "Select a batch to preview its diff.",
            Style::default().fg(TEXT_DIM),
        ))],
    };

    let para = Paragraph::new(lines)
        .block(block)
        .scroll((app.kg_review.diff_scroll, 0));
    frame.render_widget(para, area);
}

/// Colour one diff line by its leading marker.
fn diff_line(raw: &str) -> Line<'static> {
    let owned = raw.to_string();
    let color = if owned.starts_with("@@") {
        DIFF_HUNK
    } else if owned.starts_with("+++") || owned.starts_with("---") {
        TEXT_SECONDARY
    } else if owned.starts_with('+') {
        DIFF_ADD
    } else if owned.starts_with('-') {
        DIFF_DEL
    } else {
        TEXT_DIM
    };
    Line::from(Span::styled(owned, Style::default().fg(color)))
}

// ── Log view ────────────────────────────────────────────────────────────────

fn draw_log(frame: &mut Frame, app: &App, area: Rect) {
    let log = &app.kg_review.log;
    let title = format!(" Vault history ({}) ", log.len());
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(WHITE).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_set(symbols::border::ROUNDED)
        .border_style(Style::default().fg(WHITE));

    if log.is_empty() {
        let empty = Paragraph::new(Line::from(Span::styled(
            "\n  No vault history yet (git backing may be disabled).",
            Style::default().fg(TEXT_DIM),
        )))
        .block(block);
        frame.render_widget(empty, area);
        return;
    }

    let selected = app.kg_review.selected_index.min(log.len() - 1);
    let lines: Vec<Line> = log
        .iter()
        .enumerate()
        .map(|(idx, entry)| {
            let is_sel = idx == selected;
            let marker = if is_sel { "❯ " } else { "  " };
            let sha = &entry.sha[..entry.sha.len().min(8)];
            let date = entry.date.split('T').next().unwrap_or(&entry.date);
            let subj_style = if is_sel {
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT_PRIMARY)
            };
            Line::from(vec![
                Span::styled(marker, Style::default().fg(WHITE)),
                Span::styled(format!("{sha} "), Style::default().fg(ORANGE)),
                Span::styled(format!("{date}  "), Style::default().fg(TEXT_DIM)),
                Span::styled(truncate(&entry.subject, 60), subj_style),
            ])
        })
        .collect();

    let para = Paragraph::new(lines).block(block);
    frame.render_widget(para, area);
}

// ── Help bar ──────────────────────────────────────────────────────────────

fn draw_help_bar(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(Clear, area);
    let hint = if app.kg_review.confirm_restore {
        "Press r/Enter again to CONFIRM destructive restore · any other key cancels".to_string()
    } else {
        match app.kg_review.view {
            KgView::Queue => {
                "↑/↓ select · a approve · d decline · PgUp/PgDn scroll diff · Tab history · q quit"
                    .to_string()
            }
            KgView::Log => "↑/↓ select · r restore (destructive) · Tab queue · q quit".to_string(),
        }
    };
    let style = if app.kg_review.confirm_restore {
        Style::default().fg(ORANGE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(BORDER_IDLE)
    };
    frame.render_widget(Paragraph::new(Line::from(Span::styled(hint, style))), area);
}

/// Truncate to `max` chars with an ellipsis.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{kept}…")
    }
}
