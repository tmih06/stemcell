//! Activity feed data service — parses `~/.opencrabs/rsi/improvements.md`
//! into a uniform `Vec<McActivity>` for the activity panel.
//!
//! `improvements.md` is the RSI loop's append-only journal. Each entry
//! is a markdown block of the shape:
//!
//! ```md
//! ## [Applied] Add conciseness guideline
//!
//! **Date:** 2026-04-12 23:01 UTC
//! **Target:** SOUL.md
//! **Rationale:** Users consistently prefer shorter responses
//! **Status:** Applied
//! ```
//!
//! The parser is forgiving: missing fields fall back to sensible
//! defaults rather than dropping the entry.

use super::types::{McActivity, McActivityLevel};
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};

/// Read up to `limit` newest entries from the improvements log.
pub fn recent(limit: usize) -> Vec<McActivity> {
    let path = crate::config::opencrabs_home()
        .join("rsi")
        .join("improvements.md");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    parse_improvements_md(&content, limit)
}

/// Pure parser — exposed for tests.
pub fn parse_improvements_md(content: &str, limit: usize) -> Vec<McActivity> {
    let mut entries: Vec<McActivity> = Vec::new();
    let mut current: Option<EntryDraft> = None;

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            // Flush the previous entry, start a new one.
            if let Some(draft) = current.take() {
                entries.push(draft.finish());
            }
            current = Some(EntryDraft::from_header(rest));
        } else if let Some(draft) = current.as_mut() {
            apply_field_line(line, draft);
        }
    }
    if let Some(draft) = current.take() {
        entries.push(draft.finish());
    }

    // The journal is append-only with oldest at the top — surface
    // newest first so the panel matches the rest of OpenCrabs.
    entries.reverse();
    entries.truncate(limit);
    entries
}

struct EntryDraft {
    title: String,
    status_from_header: String,
    date: Option<DateTime<Utc>>,
    target: Option<String>,
    rationale: Option<String>,
    status_field: Option<String>,
}

impl EntryDraft {
    /// `header` is everything after `"## "` — typically `"[Applied] Title"`.
    fn from_header(header: &str) -> Self {
        let (status_from_header, title) = match header.strip_prefix('[') {
            Some(rest) => match rest.split_once(']') {
                Some((status, title)) => (status.trim().to_string(), title.trim().to_string()),
                None => (String::new(), header.trim().to_string()),
            },
            None => (String::new(), header.trim().to_string()),
        };
        Self {
            title,
            status_from_header,
            date: None,
            target: None,
            rationale: None,
            status_field: None,
        }
    }

    fn finish(self) -> McActivity {
        let level = level_from_status(self.status_field.as_deref(), &self.status_from_header);
        let date = self.date.unwrap_or_else(Utc::now);
        let detail = build_detail(
            &self.title,
            self.target.as_deref(),
            self.rationale.as_deref(),
        );
        McActivity {
            timestamp: date,
            detail,
            level,
            source: "rsi".to_string(),
        }
    }
}

fn apply_field_line(line: &str, draft: &mut EntryDraft) {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("**Date:**") {
        draft.date = parse_date(rest.trim());
    } else if let Some(rest) = trimmed.strip_prefix("**Target:**") {
        let target = rest.trim();
        if !target.is_empty() && target != "(none)" {
            draft.target = Some(target.to_string());
        }
    } else if let Some(rest) = trimmed.strip_prefix("**Rationale:**") {
        let rationale = rest.trim();
        if !rationale.is_empty() && rationale != "(none)" {
            draft.rationale = Some(rationale.to_string());
        }
    } else if let Some(rest) = trimmed.strip_prefix("**Status:**") {
        let s = rest.trim();
        if !s.is_empty() {
            draft.status_field = Some(s.to_string());
        }
    }
}

/// Parse the journal's `YYYY-MM-DD HH:MM UTC` format.
fn parse_date(s: &str) -> Option<DateTime<Utc>> {
    // Strip trailing " UTC" if present so the format string doesn't
    // need a hard-coded timezone token.
    let core = s.strip_suffix(" UTC").unwrap_or(s);
    NaiveDateTime::parse_from_str(core, "%Y-%m-%d %H:%M")
        .ok()
        .and_then(|naive| Utc.from_local_datetime(&naive).single())
}

fn level_from_status(status_field: Option<&str>, header_status: &str) -> McActivityLevel {
    let status = status_field.unwrap_or(header_status).to_lowercase();
    match status.as_str() {
        "applied" => McActivityLevel::Success,
        "failed" | "error" => McActivityLevel::Error,
        "warn" | "warning" | "reverted" | "rolled-back" => McActivityLevel::Warn,
        _ => McActivityLevel::Info,
    }
}

fn build_detail(title: &str, target: Option<&str>, rationale: Option<&str>) -> String {
    match (target, rationale) {
        (Some(t), Some(r)) => format!("{title} → {t} ({r})"),
        (Some(t), None) => format!("{title} → {t}"),
        (None, Some(r)) => format!("{title} ({r})"),
        (None, None) => title.to_string(),
    }
}
