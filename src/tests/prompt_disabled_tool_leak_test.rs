//! Regression tests: a tool that is NOT equipped must never be named in the
//! assembled system prompt.
//!
//! Context (2026-06-13): users reported the agent describing tools it doesn't
//! have as "disabled". Root cause was the static `BRAIN_PREAMBLE_WEB` /
//! `BRAIN_PREAMBLE_RSI` blocks: they were injected on a coarse "is any sibling
//! present" check and hardcoded every sibling's name in prose, so a tool the
//! user turned off (or that never registered) still leaked into context. The
//! model reconciled "named in prose but absent from schema" by reporting the
//! tool as disabled. The fix made both preambles data-driven — each name only
//! appears when its tool is actually in `active_tools`.

use crate::brain::prompt_builder::build_rsi_preamble;

// ── RSI preamble: data-driven ──────────────────────────────────────────────

#[test]
fn rsi_preamble_absent_when_no_rsi_tools() {
    let tools = vec!["read_file".to_string(), "bash".to_string()];
    assert!(
        build_rsi_preamble(&tools).is_none(),
        "no RSI tool equipped → no RSI preamble at all"
    );
}

#[test]
fn rsi_preamble_omits_unequipped_tools() {
    // Only feedback_analyze equipped; self_improve and feedback_record are off.
    let tools = vec!["feedback_analyze".to_string()];
    let preamble =
        build_rsi_preamble(&tools).expect("preamble present when feedback_analyze equipped");
    assert!(
        preamble.contains("feedback_analyze"),
        "equipped feedback_analyze must be named: {preamble}"
    );
    assert!(
        !preamble.contains("self_improve"),
        "unequipped self_improve must not be named: {preamble}"
    );
    assert!(
        !preamble.contains("feedback_record"),
        "unequipped feedback_record must not be named: {preamble}"
    );
}

#[test]
fn rsi_preamble_names_all_when_all_equipped() {
    let tools = vec![
        "feedback_analyze".to_string(),
        "feedback_record".to_string(),
        "self_improve".to_string(),
    ];
    let preamble =
        build_rsi_preamble(&tools).expect("preamble present when all RSI tools equipped");
    assert!(preamble.contains("feedback_analyze"));
    assert!(preamble.contains("feedback_record"));
    assert!(preamble.contains("self_improve"));
}

// ── end-to-end: disabled tools never reach the assembled prompt ─────────────

/// The user-visible contract: build the brain with a constrained tool set and
/// assert that the names of common opt-out tools never appear when they're not
/// equipped. This is the test that would have caught the original leak.
#[test]
fn assembled_prompt_never_names_unequipped_tools() {
    use crate::brain::prompt_builder::BrainLoader;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    let loader = BrainLoader::new(dir.path().to_path_buf());

    // A realistic minimal equip: file ops + one search tool + bash. No browser,
    // no RSI, no extra search tools.
    let equipped: Vec<String> = ["read_file", "write_file", "edit_file", "bash", "web_search"]
        .into_iter()
        .map(String::from)
        .collect();

    let brain = loader.build_core_brain(None, None, Some(&equipped));

    for absent in [
        "browser_navigate",
        "brave_search",
        "exa_search",
        "self_improve",
        "feedback_analyze",
        "feedback_record",
    ] {
        assert!(
            !brain.contains(absent),
            "unequipped tool `{absent}` leaked into the assembled prompt:\n{brain}"
        );
    }

    // Sanity: the equipped tools that carry prose ARE still present.
    assert!(brain.contains("web_search"));
    assert!(brain.contains("`gh` CLI"));
}
