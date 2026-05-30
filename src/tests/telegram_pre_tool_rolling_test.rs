//! Tests for the pre-tool rolling status line in Telegram.
//!
//! Regression context: commit 60a0fef1 removed the THINKING_QUIPS
//! fallback (hardcoded "Warming up the neurons" etc.) — correct fix
//! for that specific complaint, but it left the pre-tool phase
//! completely silent. Users reported that on slow turns the bot
//! shows nothing but the native "is typing" indicator, dropping the
//! rolling-status feature entirely.
//!
//! The intent was always "make rolling messages context-aware", not
//! "remove rolling messages". `pre_tool_rolling` re-introduces a
//! rolling line that's anchored to real user input (the first non-
//! empty line of the user's message) with a leading verb that
//! rotates across elapsed-time buckets so the status visibly evolves
//! while the model is taking its time.
//!
//! These tests pin three properties:
//!
//! 1. The body is always derived from the user message preview —
//!    never a hardcoded string. If `preview` is None the helper
//!    returns None (the silent path is still reachable when we
//!    genuinely have no context).
//! 2. The leading verb rotates monotonically across elapsed buckets
//!    so a user staring at the status sees actual change over time.
//! 3. The preview-builder produces a sane, length-capped, single-
//!    line string from arbitrary user input (multiline, ws-heavy,
//!    very long, unicode).

use crate::channels::telegram::handler::{build_user_message_preview, pre_tool_rolling};

#[test]
fn returns_none_before_threshold_so_typing_indicator_speaks_alone() {
    // The first ~5s of a turn are covered by the native "is typing"
    // dots; emitting a rolling message immediately would just add
    // noise. The helper must stay quiet until 5s elapsed.
    assert_eq!(pre_tool_rolling(Some("how do I add a topic?"), 0), None);
    assert_eq!(pre_tool_rolling(Some("how do I add a topic?"), 1), None);
    assert_eq!(pre_tool_rolling(Some("how do I add a topic?"), 4), None);
}

#[test]
fn returns_none_when_preview_is_missing() {
    // The whole point of the rolling line is to anchor on real
    // context. With no preview we have no honest content to show,
    // so silence is the right answer (this is the case
    // `resume_session` will always hit since it doesn't capture a
    // fresh user input).
    assert_eq!(pre_tool_rolling(None, 10), None);
    assert_eq!(pre_tool_rolling(None, 120), None);
    assert_eq!(pre_tool_rolling(Some(""), 10), None);
    assert_eq!(pre_tool_rolling(Some("   "), 10), None);
}

#[test]
fn leading_verb_rotates_across_elapsed_buckets() {
    // Same preview body, four different elapsed times → four
    // different leading phrases. This is the rolling effect.
    let preview = Some("audit the telegram fallback chain");
    let at_5 = pre_tool_rolling(preview, 5).unwrap();
    let at_15 = pre_tool_rolling(preview, 15).unwrap();
    let at_30 = pre_tool_rolling(preview, 30).unwrap();
    let at_60 = pre_tool_rolling(preview, 60).unwrap();
    let at_180 = pre_tool_rolling(preview, 180).unwrap();

    assert!(
        at_5.starts_with("Working on:"),
        "5s should use the entry-level lead: {at_5}"
    );
    assert!(
        at_15.starts_with("Still working on:"),
        "15s should escalate the lead: {at_15}"
    );
    assert!(
        at_30.starts_with("Long one"),
        "30s should call out the duration: {at_30}"
    );
    assert!(
        at_60.starts_with("Marathon mode"),
        "60s should flag the marathon: {at_60}"
    );
    assert_eq!(
        at_60, at_180,
        "the marathon bucket is the terminal one — same lead for 60s and 3m"
    );
    // All four must end with the same user-derived body so the user
    // can always see WHICH question is taking long.
    for line in [&at_5, &at_15, &at_30, &at_60] {
        assert!(
            line.ends_with(": audit the telegram fallback chain"),
            "the rolling line must always carry the preview body: {line}"
        );
    }
}

#[test]
fn preview_picks_first_non_empty_line() {
    // Multiline pastes happen all the time. We want the first real
    // line, not the trailing payload or a blank lead-in.
    let preview = build_user_message_preview("\n\n  \nhow do I add a topic?\n\nextra noise here")
        .expect("preview should be Some");
    assert_eq!(preview, "how do I add a topic?");
}

#[test]
fn preview_collapses_internal_whitespace() {
    let preview = build_user_message_preview("  audit   the    fallback     chain  ").unwrap();
    assert_eq!(preview, "audit the fallback chain");
}

#[test]
fn preview_caps_at_60_chars_with_ellipsis() {
    let long = "I need a deep walkthrough of how the telegram resume-session pipeline reconnects the streaming state after a restart";
    let preview = build_user_message_preview(long).unwrap();
    let char_count = preview.chars().count();
    assert!(
        char_count <= 61, // 60 chars + 1 ellipsis
        "preview must cap at 60 chars + ellipsis: {char_count} from {preview}"
    );
    assert!(
        preview.ends_with('…'),
        "truncated preview must end with an ellipsis: {preview}"
    );
}

#[test]
fn preview_short_input_is_unchanged() {
    assert_eq!(
        build_user_message_preview("hi crab").unwrap(),
        "hi crab",
        "short inputs must not be ellipsised"
    );
}

#[test]
fn preview_handles_unicode_safely() {
    // 60 char-count cap, NOT 60 bytes. Russian/CJK/emoji must not
    // panic at a non-char-boundary slice. Use a string long enough
    // to force truncation so we exercise the cap path.
    let cyrillic = "Привет, краб! Помоги мне понять как работает поток обновлений Telegram внутри opencrabs, пожалуйста.";
    let preview = build_user_message_preview(cyrillic).unwrap();
    assert!(preview.chars().count() <= 61);
    assert!(preview.ends_with('…'));
    // No panic = test passes; explicit prefix check just so a
    // refactor that breaks unicode boundaries surfaces here.
    assert!(preview.starts_with("Привет"));
}

#[test]
fn preview_returns_none_for_empty_or_whitespace_only_input() {
    assert_eq!(build_user_message_preview(""), None);
    assert_eq!(build_user_message_preview("   \n\n  \t  "), None);
}
