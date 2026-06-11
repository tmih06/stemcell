//! Unit tests for the model/provider search matching + highlighting helpers
//! (`src/tui/model_search.rs`). Relocated out of an inline `#[cfg(test)]` block
//! per project policy (`src/tests/AGENTS.md`).

use crate::tui::model_search::{
    highlight_spans, match_style, matches_terms, normalize_query, query_terms,
};
use ratatui::style::Style;
use ratatui::text::Span;

fn span_texts(spans: &[Span<'_>]) -> Vec<String> {
    spans.iter().map(|span| span.content.to_string()).collect()
}

/// Concatenate only the spans styled as matches (those carrying the accent fg).
fn highlighted_text(spans: &[Span<'_>]) -> String {
    let accent = match_style(Style::default());
    spans
        .iter()
        .filter(|span| span.style.fg == accent.fg)
        .map(|span| span.content.to_string())
        .collect()
}

#[test]
fn normalize_query_trims_collapses_and_lowercases() {
    assert_eq!(normalize_query("  DeepSeek   FREE  "), "deepseek free");
    assert_eq!(normalize_query("   "), "");
    assert_eq!(normalize_query("GPT-4o"), "gpt-4o");
}

#[test]
fn query_terms_splits_on_whitespace() {
    assert_eq!(query_terms("deepseek free"), vec!["deepseek", "free"]);
    assert!(query_terms("   ").is_empty());
    assert_eq!(query_terms(" Single "), vec!["single"]);
}

#[test]
fn matches_terms_requires_all_terms() {
    let terms = query_terms("deepseek free");
    // Combined haystack where the two terms live in different fields.
    assert!(matches_terms(
        &terms,
        "DeepSeek Chat deepseek-chat-free OpenRouter"
    ));
    // Missing one term -> no match.
    assert!(!matches_terms(
        &terms,
        "DeepSeek Chat deepseek-chat OpenRouter"
    ));
    // Order independent.
    let reordered = query_terms("free deepseek");
    assert!(matches_terms(
        &reordered,
        "DeepSeek Chat deepseek-chat-free OpenRouter"
    ));
}

#[test]
fn matches_terms_empty_query_matches_everything() {
    assert!(matches_terms(&[], "anything at all"));
}

#[test]
fn matches_terms_is_substring_not_subsequence() {
    let terms = query_terms("gpt");
    assert!(matches_terms(&terms, "openai/gpt-4o"));
    assert!(!matches_terms(&terms, "g p t"));
}

#[test]
fn highlight_spans_marks_each_matching_run() {
    let terms = query_terms("deep free");
    let spans = highlight_spans(
        "deepseek-free",
        &terms,
        Style::default(),
        match_style(Style::default()),
    );
    // Concatenated spans reconstruct the original text exactly.
    assert_eq!(span_texts(&spans).concat(), "deepseek-free");
    // Both matched terms are highlighted.
    assert_eq!(highlighted_text(&spans), "deepfree");
}

#[test]
fn highlight_spans_without_terms_is_single_span() {
    let spans = highlight_spans(
        "gpt-4o",
        &[],
        Style::default(),
        match_style(Style::default()),
    );
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].content.as_ref(), "gpt-4o");
}

#[test]
fn highlight_spans_handles_non_ascii_and_case() {
    // Uppercase ASCII in the text must match the lowercase query term, and the
    // multi-byte `é` must not corrupt the char-boundary bookkeeping.
    let terms = query_terms("café");
    let spans = highlight_spans(
        "Café-Café",
        &terms,
        Style::default(),
        match_style(Style::default()),
    );
    // Round-trips the multi-byte text without panicking or corrupting it,
    // preserving the original (upper) case in the emitted spans.
    assert_eq!(span_texts(&spans).concat(), "Café-Café");
    assert_eq!(highlighted_text(&spans), "CaféCafé");
}
