//! Read-time content filter for brain files.
//!
//! Issue #164 fix 4: header stubs (sections whose body is empty after a
//! manual prune or a dedup pass) bloat the LLM's view of brain context
//! without adding signal. Stripping them at READ time leaves disk
//! authoritative (writes never lose data) while keeping the loaded view
//! clean.
//!
//! Authoritative behaviour:
//! - A `##` or `###` header is "empty" if its body region — from the line
//!   after the header until the next same-level-or-higher header, or EOF —
//!   contains only blank lines, horizontal rules (`---`, `***`, `___`),
//!   table-separator rows (`| --- | --- |`), short blockquotes (`>` lines
//!   under 40 chars total), and HTML comments (`<!-- ... -->`).
//! - Body text containing the literal tokens `TBD`, `TODO`, `WIP`, or
//!   `placeholder` (case-insensitive) is NOT considered empty — those are
//!   intentional in-flight markers and the user expects them to survive.
//! - Stripping is silent at write paths (we never mutate disk through this
//!   helper) and surfaces a `Vec<String>` of stripped header names at read
//!   paths so the tool result / log can mention what was filtered.
//!
//! Opt-out: `[brain] strip_empty_sections = false` in `config.toml` makes
//! every call here a no-op. Default is enabled.

/// Result of running the empty-section filter over a brain file body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StripResult {
    /// Filtered content. Equal to the input when no sections were stripped.
    pub content: String,
    /// Stripped header lines, in document order. Each entry is the full
    /// header line (including the leading `##` / `###` markers and any
    /// trailing whitespace as it appeared in the source). Empty when
    /// nothing was stripped — callers can use this to decide whether to
    /// surface a warning.
    pub stripped_headers: Vec<String>,
}

/// Strip headers whose body region is "empty" by the rules in the module
/// doc. Returns the filtered content and the list of stripped headers.
///
/// Always allocates a fresh String (even when nothing is stripped) — the
/// extra allocation is negligible against brain-file sizes (single-digit
/// KB typical) and lets the caller treat the return value uniformly.
pub fn strip_empty_sections(content: &str) -> StripResult {
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();
    if total == 0 {
        return StripResult::default();
    }

    // Pre-compute header levels for each line. 0 means "not a header".
    let levels: Vec<u8> = lines.iter().map(|l| header_level(l)).collect();

    // For each header line, find the end of its body region (exclusive)
    // and decide whether to keep it. We compute drop ranges as
    // (start_line, end_line_exclusive) pairs over the header + body span.
    let mut drop_ranges: Vec<(usize, usize)> = Vec::new();
    let mut stripped_headers: Vec<String> = Vec::new();

    let mut i = 0;
    while i < total {
        let lvl = levels[i];
        if lvl != 0 {
            let body_start = i + 1;
            let mut body_end = total;
            for (j, level) in levels.iter().enumerate().skip(body_start) {
                if *level != 0 && *level <= lvl {
                    body_end = j;
                    break;
                }
            }
            if body_is_empty(&lines[body_start..body_end]) {
                drop_ranges.push((i, body_end));
                stripped_headers.push(lines[i].to_string());
                i = body_end;
                continue;
            }
        }
        i += 1;
    }

    if drop_ranges.is_empty() {
        return StripResult {
            content: content.to_string(),
            stripped_headers,
        };
    }

    let mut kept: Vec<&str> = Vec::with_capacity(total);
    let mut cursor = 0;
    for (start, end) in drop_ranges {
        if cursor < start {
            kept.extend_from_slice(&lines[cursor..start]);
        }
        cursor = end;
    }
    if cursor < total {
        kept.extend_from_slice(&lines[cursor..total]);
    }

    // Preserve a trailing newline iff the original had one (lines() drops it).
    let mut out = kept.join("\n");
    if content.ends_with('\n') && !out.ends_with('\n') {
        out.push('\n');
    }

    StripResult {
        content: out,
        stripped_headers,
    }
}

/// Return the markdown header level for a line: 2 for `## `, 3 for `### `,
/// 4 for `#### `, etc. Returns 0 for non-headers and for `#` (level 1
/// titles, which we treat as document-level and never strip).
///
/// We deliberately ignore level 1 (`# `) because those mark the whole-
/// file title (`# SOUL.md`) and have semantic weight regardless of body
/// emptiness. The dedup / sync paths only ever produce level 2/3 headers,
/// so this matches what the issue's reproduction described.
fn header_level(line: &str) -> u8 {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return 0;
    }
    // Count leading hashes, then require a space (so "#1" / "#hash" don't
    // count as headers).
    let hashes = trimmed.bytes().take_while(|b| *b == b'#').count();
    if !(2..=6).contains(&hashes) {
        return 0;
    }
    let rest = &trimmed[hashes..];
    if !rest.starts_with(' ') && !rest.is_empty() {
        // `##foo` without a space is not a valid ATX header — leave alone.
        return 0;
    }
    hashes as u8
}

/// True when a body region is "empty" by the issue's spec.
fn body_is_empty(body: &[&str]) -> bool {
    for line in body {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_in_flight_marker(trimmed) {
            // Intentional placeholder — body is NOT empty.
            return false;
        }
        if is_structural_only(trimmed) {
            continue;
        }
        // Anything else is real content — body is non-empty.
        return false;
    }
    true
}

/// In-flight markers the user expects to keep: TBD, TODO, WIP,
/// placeholder. Match case-insensitively as a whole word so a section
/// titled "## TODO" with body "TODO: write this section" stays put.
fn is_in_flight_marker(trimmed: &str) -> bool {
    let lower = trimmed.to_ascii_lowercase();
    for marker in ["tbd", "todo", "wip", "placeholder"] {
        if let Some(pos) = lower.find(marker) {
            let before_ok = pos == 0
                || !lower
                    .as_bytes()
                    .get(pos - 1)
                    .copied()
                    .map(|b| b.is_ascii_alphanumeric() || b == b'_')
                    .unwrap_or(false);
            let after_idx = pos + marker.len();
            let after_ok = after_idx >= lower.len()
                || !lower
                    .as_bytes()
                    .get(after_idx)
                    .copied()
                    .map(|b| b.is_ascii_alphanumeric() || b == b'_')
                    .unwrap_or(false);
            if before_ok && after_ok {
                return true;
            }
        }
    }
    false
}

/// True if the line is "structural" content that doesn't count toward
/// emptiness: horizontal rules, table-separator rows, short blockquotes,
/// HTML comments. Anything more substantive is real content.
fn is_structural_only(trimmed: &str) -> bool {
    if is_horizontal_rule(trimmed) {
        return true;
    }
    if is_table_separator(trimmed) {
        return true;
    }
    if is_html_comment_only(trimmed) {
        return true;
    }
    if trimmed.starts_with('>') && trimmed.len() < 40 {
        return true;
    }
    false
}

fn is_horizontal_rule(trimmed: &str) -> bool {
    let bytes = trimmed.as_bytes();
    if bytes.len() < 3 {
        return false;
    }
    let first = bytes[0];
    if !matches!(first, b'-' | b'*' | b'_') {
        return false;
    }
    bytes.iter().all(|b| *b == first || *b == b' ')
        && bytes.iter().filter(|b| **b == first).count() >= 3
}

fn is_table_separator(trimmed: &str) -> bool {
    if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
        return false;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    inner.split('|').all(|cell| {
        let c = cell.trim();
        !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' '))
    })
}

fn is_html_comment_only(trimmed: &str) -> bool {
    trimmed.starts_with("<!--") && trimmed.ends_with("-->")
}
