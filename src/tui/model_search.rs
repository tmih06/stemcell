//! Multi-term search matching and match highlighting for the model/provider
//! pickers (the `/models` dialog and the `/onboard` wizard).
//!
//! Matching mirrors a palette/fuzzy-finder UX: the query is split into
//! whitespace-separated terms and an item matches only when *every* term is a
//! case-insensitive substring of the item's combined search text (model id,
//! display label, provider name, ...). This lets a query like `deepseek free`
//! match a model whose id contains "deepseek" and whose label/provider
//! contains "free", regardless of term order. The matched spans are then
//! highlighted in the rendered rows so the user can see *why* a row matched.
//!
//! The approach is intentionally a plain substring match (not a fuzzy
//! subsequence) so highlighting is unambiguous and there are no surprising
//! matches — typing `gpt` never lights up `g…p…t` scattered across a name.
//!
//! Unit tests live in `src/tests/model_search_test.rs` (project policy keeps
//! tests out of inline `#[cfg(test)]` blocks).

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

/// Brand accent (orange) used to recolor matched substrings. Mirrors
/// `BRAND_GOLD` used elsewhere in the TUI.
const MATCH_COLOR: Color = Color::Rgb(215, 100, 20);

/// Normalize a raw query: trim the ends, collapse internal whitespace runs to a
/// single space, and lowercase (ASCII). Returns an empty string for blank
/// input.
pub fn normalize_query(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    let mut last_was_space = false;
    for ch in query.trim().chars() {
        if ch.is_whitespace() {
            if !last_was_space && !out.is_empty() {
                out.push(' ');
            }
            last_was_space = true;
        } else {
            out.push(ch);
            last_was_space = false;
        }
    }
    while out.ends_with(' ') {
        out.pop();
    }
    out.make_ascii_lowercase();
    out
}

/// Split a raw query into normalized, lowercased search terms. An empty or
/// whitespace-only query yields an empty list (which matches everything).
pub fn query_terms(query: &str) -> Vec<String> {
    normalize_query(query)
        .split(' ')
        .filter(|term| !term.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Returns true when every term in `terms` is a substring of `haystack`.
/// `haystack` is lowercased internally so callers can pass raw field text.
/// An empty `terms` slice matches everything.
pub fn matches_terms(terms: &[String], haystack: &str) -> bool {
    if terms.is_empty() {
        return true;
    }
    let hay = haystack.to_ascii_lowercase();
    terms.iter().all(|term| hay.contains(term.as_str()))
}

/// Style for matched substrings, derived from the row's `base` style so it
/// works on both selected (colored background) and unselected rows: the base
/// background is kept, the foreground switches to the brand accent, and
/// bold + underline are added so the match is unmistakable.
pub fn match_style(base: Style) -> Style {
    base.fg(MATCH_COLOR)
        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
}

/// Build styled spans for `text`, applying `match_style` to every character
/// that falls inside an occurrence of one of `terms` and `base_style` to the
/// rest. UTF-8 safe (groups runs on character boundaries).
///
/// When `terms` is empty the whole string is emitted as a single `base_style`
/// span, so callers can use this unconditionally with zero visual change when
/// there is no active query.
pub fn highlight_spans(
    text: &str,
    terms: &[String],
    base_style: Style,
    match_style: Style,
) -> Vec<Span<'static>> {
    if text.is_empty() {
        return vec![Span::styled(String::new(), base_style)];
    }

    let needles: Vec<&str> = terms
        .iter()
        .map(String::as_str)
        .filter(|term| !term.is_empty())
        .collect();
    if needles.is_empty() {
        return vec![Span::styled(text.to_owned(), base_style)];
    }

    // `to_ascii_lowercase` preserves byte length, so byte offsets in `lower`
    // line up exactly with `text`. Build the char-boundary table from `text`
    // to translate the byte ranges we find back into char indices.
    let lower = text.to_ascii_lowercase();
    let mut char_starts: Vec<usize> = text.char_indices().map(|(offset, _)| offset).collect();
    char_starts.push(text.len());
    let char_count = char_starts.len() - 1;

    let mut highlighted = vec![false; char_count];
    for needle in &needles {
        let mut from = 0usize;
        while from <= lower.len() {
            let Some(rel) = lower[from..].find(needle) else {
                break;
            };
            let byte_start = from + rel;
            let byte_end = byte_start + needle.len();
            let start_index = char_starts.partition_point(|&offset| offset < byte_start);
            let end_index = char_starts.partition_point(|&offset| offset < byte_end);
            for flag in highlighted
                .iter_mut()
                .take(end_index.min(char_count))
                .skip(start_index)
            {
                *flag = true;
            }
            from = byte_end;
        }
    }

    let mut spans = Vec::new();
    let mut current = String::new();
    let mut current_highlight = highlighted[0];
    for (index, ch) in text.chars().enumerate() {
        let is_match = highlighted[index];
        if index != 0 && is_match != current_highlight {
            let style = if current_highlight {
                match_style
            } else {
                base_style
            };
            spans.push(Span::styled(std::mem::take(&mut current), style));
            current_highlight = is_match;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        let style = if current_highlight {
            match_style
        } else {
            base_style
        };
        spans.push(Span::styled(current, style));
    }
    spans
}
