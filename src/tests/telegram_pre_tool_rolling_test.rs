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
fn returns_none_when_preview_is_missing_in_anchored_buckets() {
    // The 5-59s buckets exist to anchor the line on real user
    // context. With no preview those buckets stay silent — better
    // than emitting "Working on: " with an empty tail. The 60s+
    // marathon bucket is a separate shape that DOES fire without a
    // preview (covered by `marathon_bucket_fires_even_without_preview`).
    assert_eq!(pre_tool_rolling(None, 10), None);
    assert_eq!(pre_tool_rolling(None, 30), None);
    assert_eq!(pre_tool_rolling(None, 59), None);
    assert_eq!(pre_tool_rolling(Some(""), 10), None);
    assert_eq!(pre_tool_rolling(Some("   "), 10), None);
}

#[test]
fn leading_verb_rotates_across_elapsed_buckets() {
    // Same preview body, three different elapsed times in the
    // preview-anchored range → three different leading phrases. The
    // 60s+ marathon bucket is a separate (no-preview) shape — covered
    // by `marathon_bucket_rotates_through_quip_pool_instead_of_freezing`.
    let preview = Some("audit the telegram fallback chain");
    let at_5 = pre_tool_rolling(preview, 5).unwrap();
    let at_15 = pre_tool_rolling(preview, 15).unwrap();
    let at_30 = pre_tool_rolling(preview, 30).unwrap();

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
    for line in [&at_5, &at_15, &at_30] {
        assert!(
            line.ends_with(": audit the telegram fallback chain"),
            "the rolling line must always carry the preview body: {line}"
        );
    }
}

#[test]
fn marathon_bucket_rotates_through_quip_pool_instead_of_freezing() {
    // Regression for 2026-06-03: at 60s+ the line used to freeze on
    // a static "Marathon mode — still on: <preview>" for the entire
    // remaining wait (3+ minutes observed). The fix swaps the static
    // lead for a rotating pick from the project-author-original
    // TOOL_STATUS_QUIPS pool every WINDOW_SECS seconds. This test
    // pins that the line CHANGES across the marathon bucket and that
    // every pick is from the canonical pool — never invented copy.
    use crate::channels::telegram::rolling_status_quips::TOOL_STATUS_QUIPS;

    let preview = Some("audit the telegram fallback chain");
    let mut seen: Vec<String> = Vec::new();
    // Sample across ~4 minutes of marathon time. With WINDOW_SECS=15
    // and a 15-entry pool this samples every entry at least once.
    for t in [60u64, 75, 90, 105, 120, 135, 150, 165, 180, 195, 210, 225] {
        let line = pre_tool_rolling(preview, t).unwrap();
        assert!(
            TOOL_STATUS_QUIPS.contains(&line.as_str()),
            "marathon-bucket line must be a verbatim entry from the \
             project-author-original quip pool, got: {line:?}"
        );
        seen.push(line);
    }
    // Same input → same output (deterministic rotation), so the
    // same 15s window at 60s+ produces the same quip on every tick.
    assert_eq!(
        pre_tool_rolling(preview, 60).unwrap(),
        pre_tool_rolling(preview, 74).unwrap(),
        "ticks inside the same 15s window must stay on the same quip — \
         a different one every 2s tick would be jittery"
    );
    // At least 3 different quips across the 4-minute sample — proves
    // it's not stuck on a single entry for the whole marathon.
    let unique: std::collections::HashSet<&String> = seen.iter().collect();
    assert!(
        unique.len() >= 3,
        "expected at least 3 distinct quips across 60s..225s, got {} unique: {seen:?}",
        unique.len()
    );
}

#[test]
fn marathon_bucket_fires_even_without_preview() {
    // No preview → silence in the preview-anchored buckets, but
    // marathon mode still shows a quip. Without this guard a turn
    // where the user's message preview couldn't be built (empty
    // payload, all-whitespace, resume_session reconnects) would
    // stay completely silent past 60s, leaving the user with no
    // signal that the agent is still alive.
    let line = pre_tool_rolling(None, 120)
        .expect("marathon bucket must produce a line even with no preview");
    use crate::channels::telegram::rolling_status_quips::TOOL_STATUS_QUIPS;
    assert!(
        TOOL_STATUS_QUIPS.contains(&line.as_str()),
        "no-preview marathon line must still be from the canonical pool, got: {line:?}"
    );
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
