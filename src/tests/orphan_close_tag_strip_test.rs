//! Pin the orphan-close-tag stripper added 2026-05-28 after the user
//! reported `</tool_result>` rendering as visible text in the TUI between
//! paragraphs of normal prose.
//!
//! The existing strip_xml_tool_calls regex only matched MATCHED PAIRS
//! (e.g. `<tool_call>...</tool_call>`). Models routinely emit standalone
//! close tags when the opener was eaten by an earlier pass or never
//! produced — those leaked straight to the chat surface.

use crate::utils::sanitize::strip_llm_artifacts;

// ─── User's verbatim leak shape ───────────────────────────────────────

#[test]
fn user_reported_orphan_close_tool_result() {
    // Verbatim from the 2026-05-28 user report.
    let text = "</tool_result>\n\
                Now let me verify it compiles and runs clean through clippy:\n\
                </tool_result>\n\
                Clean build, zero warnings. Here's what landed:";
    let cleaned = strip_llm_artifacts(text);
    assert!(
        !cleaned.contains("</tool_result>"),
        "orphan </tool_result> must be stripped, got: {cleaned:?}"
    );
    assert!(
        cleaned.contains("Now let me verify"),
        "prose between orphans must survive: {cleaned:?}"
    );
    assert!(
        cleaned.contains("Clean build, zero warnings"),
        "trailing prose must survive: {cleaned:?}"
    );
}

#[test]
fn orphan_close_tool_call() {
    let text = "Some output.\n</tool_call>\nMore prose.";
    let cleaned = strip_llm_artifacts(text);
    assert!(!cleaned.contains("</tool_call>"));
    assert!(cleaned.contains("Some output"));
    assert!(cleaned.contains("More prose"));
}

#[test]
fn orphan_close_invoke() {
    let text = "Result data.\n</invoke>\nNext step.";
    let cleaned = strip_llm_artifacts(text);
    assert!(!cleaned.contains("</invoke>"));
}

#[test]
fn orphan_close_function_calls() {
    let text = "Trying again.\n</function_calls>\nDone.";
    let cleaned = strip_llm_artifacts(text);
    assert!(!cleaned.contains("</function_calls>"));
}

#[test]
fn orphan_close_qwen_namespaced() {
    let text = "Running.\n</qwen:tool_call>\nFinished.";
    let cleaned = strip_llm_artifacts(text);
    assert!(!cleaned.contains("</qwen:tool_call>"));
}

#[test]
fn orphan_close_tool_use() {
    let text = "Done.\n</tool_use>\nNext.";
    let cleaned = strip_llm_artifacts(text);
    assert!(!cleaned.contains("</tool_use>"));
}

// ─── Negative cases — prose mentions must survive ─────────────────────

#[test]
fn prose_mention_of_close_tag_mid_sentence_survives() {
    // A mid-sentence reference to the tag (no surrounding whitespace
    // dedicated to it) is prose — must not be eaten.
    let text = "We had a bug where </tool_result> appeared inline in chat.";
    let cleaned = strip_llm_artifacts(text);
    assert!(
        cleaned.contains("</tool_result>"),
        "mid-sentence prose mention must survive: {cleaned:?}"
    );
}

#[test]
fn matched_pair_still_stripped_with_content() {
    // Confirm the matched-pair regex still works alongside the orphan
    // stripper — this test exists to catch a regression where the new
    // unconditional orphan pass accidentally swallows real matched
    // pairs first.
    let text = "Header.\n<tool_call>{\"name\":\"bash\",\"arguments\":{\"command\":\"ls\"}}</tool_call>\nFooter.";
    let cleaned = strip_llm_artifacts(text);
    assert!(!cleaned.contains("<tool_call>"));
    assert!(!cleaned.contains("</tool_call>"));
    assert!(cleaned.contains("Header"));
    assert!(cleaned.contains("Footer"));
}

#[test]
fn no_op_when_no_xml_tags_present() {
    let text = "Just normal prose with no tags at all.";
    let cleaned = strip_llm_artifacts(text);
    assert_eq!(cleaned, text, "plain prose must round-trip unchanged");
}
