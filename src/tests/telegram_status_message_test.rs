//! Tests for `thinking_status_excerpt` — extracts a short
//! status-line snippet from the agent's live reasoning text so the
//! Telegram status message reflects what the agent is actually
//! focused on, instead of showing a hardcoded "Thinking through
//! this..." filler.
//!
//! Before this fix the pre-tool reasoning phase always rendered the
//! same string regardless of what the agent was streaming. The
//! screenshot on 2026-05-30 01:43 showed the reasoning text
//! ("I am assessing the existing data model...") streaming above
//! the status line which said "Thinking through this..." — the
//! status line ignored the reasoning content right above it.

use crate::channels::telegram::handler::thinking_status_excerpt;

#[test]
fn empty_reasoning_returns_none() {
    assert_eq!(thinking_status_excerpt(""), None);
}

#[test]
fn whitespace_only_returns_none() {
    assert_eq!(thinking_status_excerpt("   \n\t  "), None);
}

#[test]
fn too_short_reasoning_returns_none() {
    // Less than 20 chars is just a stub; fall back to the quip
    // rotation instead of showing "Hi." as a status.
    assert_eq!(thinking_status_excerpt("Hello."), None);
}

#[test]
fn picks_last_complete_sentence() {
    let input = "First I will check the README. Then I will run the tests. Finally I will report.";
    let result = thinking_status_excerpt(input).unwrap();
    assert!(
        result.contains("report") || result.starts_with("Finally"),
        "must pick the latest sentence; got: {result}"
    );
}

#[test]
fn strips_first_person_lead_in_phrases() {
    let input = "I am assessing the existing data model and user interface.";
    let result = thinking_status_excerpt(input).unwrap();
    assert!(
        !result.starts_with("I am"),
        "should strip `I am`; got: {result}"
    );
    assert!(
        result.starts_with("Assessing"),
        "should capitalise after strip; got: {result}"
    );
}

#[test]
fn strips_im_contraction() {
    let input = "I'm comparing this against the spec.";
    let result = thinking_status_excerpt(input).unwrap();
    assert!(!result.starts_with("I'm"));
    assert!(result.starts_with("Comparing"));
}

#[test]
fn strips_let_me_lead_in() {
    let input = "Let me check the relevant configuration before proceeding.";
    let result = thinking_status_excerpt(input).unwrap();
    assert!(result.starts_with("Check"));
}

#[test]
fn strips_i_will_lead_in() {
    let input = "I will gather the latest data from the upstream feed.";
    let result = thinking_status_excerpt(input).unwrap();
    assert!(result.starts_with("Gather"));
}

#[test]
fn caps_at_80_chars_with_ellipsis() {
    let input = "I am working through a very long and detailed plan that covers many separate considerations all at once and should definitely overflow the eighty character status line cap.";
    let result = thinking_status_excerpt(input).unwrap();
    assert!(
        result.chars().count() <= 81,
        "must cap at ~80 chars; got {} chars: {}",
        result.chars().count(),
        result
    );
    assert!(
        result.ends_with('…'),
        "should mark truncation; got: {result}"
    );
}

#[test]
fn short_complete_sentence_is_not_truncated() {
    let input = "I am loading the configuration.";
    let result = thinking_status_excerpt(input).unwrap();
    assert_eq!(result, "Loading the configuration");
    assert!(!result.ends_with('…'));
}

#[test]
fn unicode_first_char_capitalises_correctly() {
    // French / Spanish reasoning is common in the user's groups.
    let input = "I am évaluant les options possibles avant de continuer.";
    let result = thinking_status_excerpt(input).unwrap();
    assert!(result.starts_with('É'));
}

#[test]
fn multi_line_reasoning_picks_last_sentence_across_newlines() {
    let input = "Step one: read the file.\nStep two: parse the JSON.\nStep three: validate against the schema.";
    let result = thinking_status_excerpt(input).unwrap();
    assert!(
        result.contains("validate") || result.contains("schema"),
        "must pick text after the last newline; got: {result}"
    );
}

#[test]
fn fragment_only_pre_punctuation_still_extracts() {
    // The reasoning is still streaming — no terminal punctuation yet.
    let input = "I am loading the documents and preparing to summarise them";
    let result = thinking_status_excerpt(input).unwrap();
    assert!(result.starts_with("Loading"), "got: {result}");
}

#[test]
fn ignores_trivial_intermediate_sentences() {
    // "OK." is too short, filter discards it, so we pick the
    // sentence before. Otherwise users would see one-character
    // status updates that say nothing.
    let input = "I am analyzing the user request carefully. OK.";
    let result = thinking_status_excerpt(input).unwrap();
    assert!(result.starts_with("Analyzing"), "got: {result}");
}

#[test]
fn realistic_screenshot_scenario() {
    // Verbatim from the screenshot that triggered this fix.
    let input = "I am assessing the existing data model and user interface to understand what information is currently available.";
    let result = thinking_status_excerpt(input).unwrap();
    assert!(result.starts_with("Assessing the existing data model"));
}
