//! Sentinel tests for the "do not emit IDE-style inline edits"
//! directive in BRAIN_PREAMBLE.
//!
//! Root cause of the 2026-05-30 14:14 incident: qwen-3.7-max-thinking
//! emitted ```` ```dart|CODE_EDIT_BLOCK|/abs/path/file.dart ````
//! fenced blocks containing full file contents, expecting an
//! IDE (Cursor) to apply the edit. We strip those blocks on the
//! output side (`utils::sanitize::strip_code_edit_block_fences`),
//! but the prompt also needs to teach the model not to emit them
//! in the first place — stripping is the safety net, the
//! directive is the cure.
//!
//! These sentinels fail loudly if a future refactor drops the
//! directive, so the strip pass doesn't silently become the
//! only line of defence.

use crate::brain::prompt_builder::BRAIN_PREAMBLE_CORE;

#[test]
fn forbids_cursor_style_code_edit_block_format() {
    assert!(
        BRAIN_PREAMBLE_CORE.contains("CODE_EDIT_BLOCK"),
        "preamble must explicitly name the Cursor-style CODE_EDIT_BLOCK \
         marker so the model recognises and avoids it"
    );
}

#[test]
fn forbids_aider_style_search_replace_markers() {
    assert!(
        BRAIN_PREAMBLE_CORE.contains("SEARCH") && BRAIN_PREAMBLE_CORE.contains("REPLACE"),
        "preamble must call out Aider-style <<<<<<< SEARCH / >>>>>>> REPLACE \
         markers — same failure mode as CODE_EDIT_BLOCK"
    );
}

#[test]
fn points_agent_at_edit_file_tool_as_the_real_path() {
    assert!(
        BRAIN_PREAMBLE_CORE.contains("edit_file"),
        "after forbidding inline formats, preamble must point the agent \
         at the actual `edit_file` tool — otherwise the model has nowhere to go"
    );
}

#[test]
fn warns_about_file_content_leak() {
    // The agent needs to know WHY the inline format is forbidden —
    // not just "don't do it" but "doing it leaks file contents to
    // the channel". Models follow rules better when they understand
    // the consequence.
    let lower = BRAIN_PREAMBLE_CORE.to_lowercase();
    assert!(
        lower.contains("leak") || lower.contains("expose"),
        "preamble must explain the failure mode (file contents leak to \
         the channel) so the model treats the rule as load-bearing"
    );
}

#[test]
fn lists_concrete_forbidden_patterns() {
    // The directive lists specific patterns the model has been
    // trained on. If a future edit collapses the list into a vague
    // "don't emit inline edits" the model loses the recognisable
    // anchors that make the rule actionable.
    let count = ["CODE_EDIT_BLOCK", "SEARCH", "REPLACE", "diff"]
        .iter()
        .filter(|p| BRAIN_PREAMBLE_CORE.contains(*p))
        .count();
    assert!(
        count >= 3,
        "preamble must name at least 3 concrete inline-edit patterns \
         (CODE_EDIT_BLOCK, SEARCH/REPLACE, diff headers) so the model \
         recognises whichever one its fine-tune dataset taught it"
    );
}
