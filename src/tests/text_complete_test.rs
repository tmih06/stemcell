//! Tests for `text_looks_complete` — the heuristic that lets us
//! distinguish "stream truly truncated" from "provider doesn't send
//! [DONE] but the response is fine".
//!
//! Regression context: 2026-05-30, user observed "StemCell is
//! responding... (491s · 2182 tok)" with the response already
//! fully rendered in the chat. Root cause: dialagram +
//! qwen-3.7-max-thinking closes the TCP stream without `[DONE]`
//! after delivering a complete response. Our pipeline treated
//! that as a failure, retried 3 times, then fell back — pegging
//! the indicator for 8 minutes.
//!
//! After the fix, completed-looking responses synthesise
//! StopReason::EndTurn so the loop exits cleanly. These tests
//! pin both directions:
//!   - obviously-complete text → true (no retry).
//!   - obviously-truncated text → false (retry).

use crate::utils::text_complete::text_looks_complete;

fn pad(text: &str) -> String {
    // The complete-check requires at least 200 chars of substance
    // before declaring complete. Most realistic responses easily
    // clear that, but unit tests benefit from focused short
    // snippets — pad with realistic prose so we exercise the
    // ending heuristic, not the length floor.
    let filler = "This is filler prose that brings the response above the minimum length threshold. \
                  The model wrote real content earlier in the response and we want to focus the \
                  assertion on how the text ends, not how long it is. ";
    format!("{filler}{text}")
}

// ── Sentence terminators (complete) ─────────────────────────────

#[test]
fn ends_with_period_is_complete() {
    assert!(text_looks_complete(&pad("This is a final sentence.")));
}

#[test]
fn ends_with_question_mark_is_complete() {
    assert!(text_looks_complete(&pad("Is this complete?")));
}

#[test]
fn ends_with_exclamation_is_complete() {
    assert!(text_looks_complete(&pad("Done!")));
}

#[test]
fn ends_with_closing_quote_is_complete() {
    assert!(text_looks_complete(&pad("She said \"yes\"")));
}

#[test]
fn ends_with_closing_paren_is_complete() {
    assert!(text_looks_complete(&pad("Available actions (see docs)")));
}

#[test]
fn ends_with_ellipsis_unicode_is_complete() {
    // \u{2026} is the proper ellipsis char — often used at end of
    // a deliberate "I'll get back to you …" type response.
    assert!(text_looks_complete(&pad(
        "Waiting for the upstream\u{2026}"
    )));
}

#[test]
fn ends_with_inline_code_backtick_is_complete() {
    assert!(text_looks_complete(&pad("Use `cargo test`")));
}

// ── Mid-sentence markers (truncated) ────────────────────────────

#[test]
fn ends_with_colon_is_truncated() {
    // Classic truncation pattern: "Let me check the README:" cut
    // before the list contents arrived.
    assert!(!text_looks_complete(&pad("Let me check the README:")));
}

#[test]
fn ends_with_comma_is_truncated() {
    assert!(!text_looks_complete(&pad("The options include foo, bar,")));
}

#[test]
fn ends_with_semicolon_is_truncated() {
    assert!(!text_looks_complete(&pad("Three statements;")));
}

#[test]
fn ends_with_em_dash_is_truncated() {
    assert!(!text_looks_complete(&pad("Considering the next step -")));
}

#[test]
fn ends_with_open_paren_is_truncated() {
    assert!(!text_looks_complete(&pad("See the docs (")));
}

// ── Structural / length checks ──────────────────────────────────

#[test]
fn empty_text_is_not_complete() {
    assert!(!text_looks_complete(""));
}

#[test]
fn short_text_below_threshold_is_not_complete() {
    // Even with terminal punctuation, a 20-char response is
    // almost certainly a truncated preamble — retry rather than
    // accept.
    assert!(!text_looks_complete("All done."));
}

#[test]
fn whitespace_only_is_not_complete() {
    assert!(!text_looks_complete(&format!("   {}   ", "\n".repeat(10))));
}

// ── Code fence balance ──────────────────────────────────────────

#[test]
fn unclosed_code_fence_is_truncated() {
    // Model started a fence and the stream cut before it closed —
    // the rendered output would visually leak the fence into chat.
    // Retry instead.
    let text = format!(
        "{}\n```rust\nfn main() {{ let x = 1;",
        "Here is some leading prose that exceeds the minimum length threshold. ".repeat(4)
    );
    assert!(!text_looks_complete(&text));
}

#[test]
fn closed_code_fence_is_complete() {
    let text = format!(
        "{}\n```rust\nfn main() {{ let x = 1; }}\n```",
        "Here is some leading prose that exceeds the minimum length threshold. ".repeat(4)
    );
    assert!(text_looks_complete(&text));
}

#[test]
fn multiple_balanced_code_fences_are_complete() {
    let text = format!(
        "{}\n```rust\nlet a = 1;\n```\n\nAnd another:\n```rust\nlet b = 2;\n```",
        "Here is some leading prose that exceeds the minimum length threshold. ".repeat(3)
    );
    assert!(text_looks_complete(&text));
}

// ── Realistic regression cases ──────────────────────────────────

#[test]
fn ends_with_word_only_is_treated_as_complete_when_long_enough() {
    // Default case: ends with a normal word, no terminator, no
    // truncation marker. The heuristic biases toward "don't
    // retry" because spurious retries are visible to the user
    // (the "responding" timer climbs) while accepting an
    // occasionally informal-ending response is harmless.
    let text = "Here is a long body of text where the model ended without a final \
                punctuation mark but the response is otherwise coherent and addresses \
                what was asked. The user can read it and the conversation continues \
                from there without any visible glitch."
        .to_string();
    assert!(text_looks_complete(&text));
}

#[test]
fn regression_screenshot_changelog_response() {
    // Mirrors the actual incident: a long CHANGELOG-style response
    // ending with "Ready to draft the CHANGELOG entry when you give
    // the word." — clearly complete, must NOT trigger retry.
    let text = "## v0.3.31 Breakdown\n\
                **37 commits across 10 categories.** Much bigger than the v0.3.31 draft \
                I had prepped earlier (which only covered 20 commits). The new additions \
                are: the sanitization fix, RSI subsystem classifier + skill proposal kind, \
                CI/build hardening, usage dashboard consolidation, IDE-format ban in \
                prompt, THINKING_QUIPS removal, and forum topic name capture.\n\n\
                Ready to draft the CHANGELOG entry when you give the word.";
    assert!(text_looks_complete(text));
}

#[test]
fn regression_truncated_preamble() {
    // The OTHER side: classic truncation. "Let me check the
    // README:" with nothing after it — must retry.
    let text = format!(
        "{}\n\nLet me check the README:",
        "I'll start by reading through the existing documentation to understand the structure. \
         I want to confirm the patterns before making changes. "
            .repeat(3)
    );
    assert!(!text_looks_complete(&text));
}
