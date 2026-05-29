//! Direct unit tests for `parse_page_range` — the page-spec parser
//! that backs the `page_range` argument on `parse_document`. This
//! function is what saves the agent from page-by-page tool calls
//! when reading long PDFs; if it returns the wrong page set the
//! agent gets text from the wrong pages and reasons about wrong
//! content. Every edge case below has a plausible failure mode in
//! the field — the model emits trailing commas, mixed separators,
//! reversed ranges, leading zeros — and the parser is intentionally
//! lenient.

use crate::brain::tools::doc_parser::parse_page_range;

#[test]
fn single_page_number() {
    assert_eq!(parse_page_range("5"), vec![5]);
}

#[test]
fn simple_range() {
    assert_eq!(parse_page_range("1-3"), vec![1, 2, 3]);
}

#[test]
fn large_range_spans_correctly() {
    let result = parse_page_range("1-30");
    assert_eq!(result.len(), 30);
    assert_eq!(result.first(), Some(&1));
    assert_eq!(result.last(), Some(&30));
}

#[test]
fn comma_separated_singles() {
    assert_eq!(parse_page_range("1,3,5"), vec![1, 3, 5]);
}

#[test]
fn mixed_singles_and_ranges() {
    assert_eq!(
        parse_page_range("5,7,10-15"),
        vec![5, 7, 10, 11, 12, 13, 14, 15]
    );
}

#[test]
fn whitespace_after_commas_is_ok() {
    assert_eq!(parse_page_range("1, 2, 3"), vec![1, 2, 3]);
}

#[test]
fn whitespace_only_separator() {
    assert_eq!(parse_page_range("1 2 3"), vec![1, 2, 3]);
}

#[test]
fn whitespace_around_range_dash() {
    // Model sometimes emits `"1 - 5"` instead of `"1-5"`.
    assert_eq!(parse_page_range("1 - 5"), vec![1, 2, 3, 4, 5]);
}

#[test]
fn duplicate_pages_are_deduped() {
    assert_eq!(parse_page_range("1,1,2,2,3"), vec![1, 2, 3]);
}

#[test]
fn overlapping_ranges_are_deduped() {
    assert_eq!(parse_page_range("1-3,2-4"), vec![1, 2, 3, 4]);
}

#[test]
fn output_is_sorted() {
    assert_eq!(parse_page_range("5,1,3,2,4"), vec![1, 2, 3, 4, 5]);
}

#[test]
fn ranges_combined_with_singles_sort() {
    assert_eq!(parse_page_range("10-12,1,5"), vec![1, 5, 10, 11, 12]);
}

#[test]
fn empty_string_returns_empty() {
    assert_eq!(parse_page_range(""), Vec::<usize>::new());
}

#[test]
fn only_whitespace_returns_empty() {
    assert_eq!(parse_page_range("   "), Vec::<usize>::new());
}

#[test]
fn only_commas_returns_empty() {
    assert_eq!(parse_page_range(",,,"), Vec::<usize>::new());
}

#[test]
fn page_zero_is_rejected() {
    // Pages are 1-indexed; 0 is meaningless and silently dropped.
    assert_eq!(parse_page_range("0"), Vec::<usize>::new());
    assert_eq!(parse_page_range("0,1"), vec![1]);
    assert_eq!(parse_page_range("0-5"), Vec::<usize>::new());
}

#[test]
fn reversed_range_is_rejected() {
    // "5-3" doesn't unambiguously mean either direction; drop it.
    assert_eq!(parse_page_range("5-3"), Vec::<usize>::new());
}

#[test]
fn reversed_range_does_not_taint_valid_neighbours() {
    // The agent might emit `"1-3, 5-3, 7"` mid-thought; the valid
    // tokens must still come through.
    assert_eq!(parse_page_range("1-3,5-3,7"), vec![1, 2, 3, 7]);
}

#[test]
fn non_numeric_tokens_are_dropped() {
    assert_eq!(parse_page_range("abc"), Vec::<usize>::new());
    assert_eq!(parse_page_range("1,abc,2"), vec![1, 2]);
    assert_eq!(parse_page_range("first-third"), Vec::<usize>::new());
}

#[test]
fn negative_numbers_are_dropped() {
    // `"-5"` is not a valid page; `usize::from_str("-5")` fails.
    assert_eq!(parse_page_range("-5"), Vec::<usize>::new());
    assert_eq!(parse_page_range("-1,2,3"), vec![2, 3]);
}

#[test]
fn malformed_range_with_one_side_missing() {
    // `"5-"` parses to (Some(5), Err) → dropped.
    assert_eq!(parse_page_range("5-"), Vec::<usize>::new());
    assert_eq!(parse_page_range("-5"), Vec::<usize>::new());
}

#[test]
fn single_page_range() {
    // Range where start == end is just that page.
    assert_eq!(parse_page_range("7-7"), vec![7]);
}

#[test]
fn very_large_page_number() {
    // 5-digit page numbers (e.g. legal discovery PDFs).
    assert_eq!(parse_page_range("99999"), vec![99999]);
    assert_eq!(parse_page_range("99998-99999"), vec![99998, 99999]);
}

#[test]
fn realistic_agent_input_pages_31_to_60_after_truncation() {
    // The exact case the new feature enables: after the inline
    // preview shows pages 1-30, the agent asks for the next 30.
    let result = parse_page_range("31-60");
    assert_eq!(result.len(), 30);
    assert_eq!(result[0], 31);
    assert_eq!(result[29], 60);
}

#[test]
fn realistic_agent_input_with_summary_pages() {
    // Agent wants the executive summary (pages 1-3) plus the
    // conclusion (page 32) of a 32-page report.
    assert_eq!(parse_page_range("1-3,32"), vec![1, 2, 3, 32]);
}
