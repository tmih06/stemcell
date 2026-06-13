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

    let brain = loader.build_core_brain(None, Some(&equipped));

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

/// The brain-file access verbs (`load_brain_file` / `write_stemcell_file`) are
/// named in the "Available Context Files" section, which renders whenever brain
/// files exist on disk. They must be gated on the tools being *equipped*, not on
/// the files existing — otherwise a build without those tools name-drops them
/// (the agent then reports them "disabled" or ghost-calls them).
#[test]
fn brain_file_section_omits_unequipped_access_tools() {
    use crate::brain::prompt_builder::BrainLoader;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    // Brain files on disk → the section WILL render.
    std::fs::write(dir.path().join("MEMORY.md"), "notes").unwrap();
    let loader = BrainLoader::new(dir.path().to_path_buf());

    // File ops only — neither load_brain_file nor write_stemcell_file equipped.
    let equipped: Vec<String> = ["read_file", "edit_file", "bash"]
        .into_iter()
        .map(String::from)
        .collect();
    let brain = loader.build_core_brain(None, Some(&equipped));

    // Section still renders (files exist) and lists MEMORY.md…
    assert!(
        brain.contains("Available Context Files"),
        "section must render when brain files exist:\n{brain}"
    );
    // …but the unequipped access tools must NOT be named.
    assert!(
        !brain.contains("load_brain_file"),
        "unequipped load_brain_file leaked into brain-file section:\n{brain}"
    );
    assert!(
        !brain.contains("write_stemcell_file"),
        "unequipped write_stemcell_file leaked into brain-file section:\n{brain}"
    );
}

/// Inverse of the above: when the access tools ARE equipped, they must be named
/// so the agent knows the verb for loading/writing brain files.
#[test]
fn brain_file_section_names_equipped_access_tools() {
    use crate::brain::prompt_builder::BrainLoader;
    use tempfile::TempDir;

    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("MEMORY.md"), "notes").unwrap();
    let loader = BrainLoader::new(dir.path().to_path_buf());

    let equipped: Vec<String> = ["read_file", "load_brain_file", "write_stemcell_file"]
        .into_iter()
        .map(String::from)
        .collect();
    let brain = loader.build_core_brain(None, Some(&equipped));

    assert!(
        brain.contains("load_brain_file"),
        "equipped load_brain_file must be named so the agent knows the load verb:\n{brain}"
    );
    assert!(
        brain.contains("write_stemcell_file"),
        "equipped write_stemcell_file must be named:\n{brain}"
    );
}
