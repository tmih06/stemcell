//! Regression tests for the Qwen 3 / DeepSeek SentencePiece-style
//! tool-call marker leak.
//!
//! Observed: when a Qwen 3 model (via custom OpenAI-compatible
//! endpoint) is asked to call a tool, it emits the proper
//! structured `tool_calls` field AND a human-readable token-marker
//! echo in `content`:
//!
//!   <|tool▁calls_section_begin|>
//!   <|tool▁call_begin|>analyze_video<|tool▁calls_section_end|>
//!   <|tool▁calls_section_end|> ... (hundreds of repeats)
//!
//! The streaming filter at `custom_openai_compatible.rs` only knew
//! `<tool_call>` and `<function=` — Qwen's `<|tool▁...|>` family
//! was unknown, so every chunk streamed straight to the user's
//! channel as text. The repetition guard at `helpers.rs:320` did
//! eventually kill the stream, but only after ~7 KB of garbage had
//! already shipped to Telegram and the TUI and been persisted as
//! the assistant turn.
//!
//! Two surgical fixes pin this:
//!
//! 1. `TOOL_MARKERS` (streaming) — add the `<|tool▁calls_section_begin|>`
//!    and `<|tool▁call_begin|>` openers so the filter routes the
//!    whole section into the tool-capture buffer instead of display.
//! 2. `strip_llm_artifacts` (post-stream) — sweep any
//!    `<|tool<sep>...|>` token that survived the streaming filter,
//!    where `<sep>` is U+2581 OR ASCII `_` (some quantizations emit
//!    the ASCII form).
//!
//! These tests pin both ends.

use crate::utils::sanitize::strip_llm_artifacts;

const SEP: &str = "\u{2581}"; // SentencePiece word-boundary char emitted by Qwen 3

#[test]
fn strips_qwen_section_open_and_close_with_u2581() {
    // All four marker variants are stripped, but the literal tool
    // name between `call_begin` and `call_end` is preserved — by
    // design, so prose mentioning the tool name elsewhere doesn't
    // get clobbered. See `preserves_tool_name_payload_between_markers`.
    let input = format!(
        "Hello<|tool{SEP}calls_section_begin|><|tool{SEP}call_begin|>analyze_video<|tool{SEP}call_end|><|tool{SEP}calls_section_end|>"
    );
    let out = strip_llm_artifacts(&input);
    assert!(
        !out.contains("<|tool"),
        "no marker fragment must survive: {out:?}"
    );
    assert!(out.starts_with("Hello"), "prose prefix preserved: {out:?}");
    assert!(out.contains("analyze_video"), "payload preserved: {out:?}");
}

#[test]
fn strips_repeated_section_end_storm() {
    // The actual incident pattern: one call_begin + analyze_video,
    // then hundreds of section_end repeats from the model loop.
    let mut input = String::from("prefix");
    input.push_str(&format!("<|tool{SEP}calls_section_begin|>"));
    input.push_str(&format!("<|tool{SEP}call_begin|>"));
    input.push_str("analyze_video");
    for _ in 0..50 {
        input.push_str(&format!("<|tool{SEP}calls_section_end|>"));
    }
    input.push_str("suffix");
    let out = strip_llm_artifacts(&input);
    assert_eq!(
        out, "prefixanalyze_videosuffix",
        "every marker in the storm must go, leaving only the literal tool name + prose"
    );
    assert!(
        !out.contains("<|tool"),
        "no marker fragment should survive: {out:?}"
    );
}

#[test]
fn strips_ascii_underscore_variant() {
    // Some Qwen quantizations / forks emit ASCII `_` instead of
    // U+2581. The regex must catch both so we don't regress when
    // the next quantization ships.
    let input =
        "before<|tool_calls_section_begin|><|tool_call_begin|>grep<|tool_calls_section_end|>after";
    let out = strip_llm_artifacts(input);
    assert_eq!(out, "beforegrepafter");
}

#[test]
fn strips_unknown_future_variants_via_catchall() {
    // The regex is intentionally `<\|tool[<sep>][^|]*\|>` so a
    // future variant like `<|tool▁call_argument_begin|>` is
    // caught without a code change.
    let input = format!("a<|tool{SEP}call_argument_begin|>b<|tool{SEP}call_metadata|>c");
    let out = strip_llm_artifacts(&input);
    assert_eq!(
        out, "abc",
        "future marker variants must be swept too: {out:?}"
    );
}

#[test]
fn preserves_prose_that_only_looks_like_a_marker() {
    // The leading `<|tool` followed by U+2581-or-`_` is specific
    // enough that natural prose never matches.
    let input = "the <|tool of choice|> for this is rg";
    let out = strip_llm_artifacts(input);
    assert_eq!(
        out, input,
        "must NOT strip prose that lacks the SP/underscore separator"
    );
}

#[test]
fn preserves_tool_name_payload_between_markers() {
    // The model's intent (the literal tool name `analyze_video`)
    // survives — only the marker wrappers go. This matters because
    // some downstream sanitizers / displays may want to surface
    // the tool name; we strip wrappers, not content.
    let input = format!("<|tool{SEP}call_begin|>analyze_video<|tool{SEP}call_end|>");
    let out = strip_llm_artifacts(&input);
    assert_eq!(out, "analyze_video");
}

#[test]
fn handles_input_with_no_markers_fast_path() {
    // `strip_llm_artifacts` guards the regex behind a
    // `result.contains("<|tool")` check so the common case (no
    // markers) doesn't allocate a regex scan.
    let input = "regular agent response with no Qwen tokens at all";
    let out = strip_llm_artifacts(input);
    assert_eq!(out, input);
}
