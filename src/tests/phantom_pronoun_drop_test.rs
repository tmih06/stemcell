//! Tests for the pronoun-dropped intent patterns added 2026-06-01.
//!
//! User reported a model narration that wasn't caught by the phantom
//! detector: "Need to read the tool_loop_inner function entry to add
//! turn timing. Let me check that first." The existing intent_phrases
//! list covered "i need to read" (with the explicit pronoun) but not
//! "need to read" (model dropped the leading "I" in telegraphic
//! narration style).
//!
//! Pronounless variants now cover EN ("need to X") and FR ("besoin
//! de X" — the "j'ai" elision). ES / PT verb forms are already
//! first-person via conjugation (no possible elision). RU already
//! shipped pronounless variants for the core verbs.
//!
//! These tests pin the additions so a refactor that collapses the
//! intent_phrases list can't silently drop them and re-open the
//! "Need to X" detection gap.

use crate::brain::agent::service::has_phantom_tool_intent_no_tools;

#[test]
fn detects_pronounless_need_to_read_user_exact_leak() {
    // The literal text from the 2026-06-01 user report. Must fire
    // the phantom detector — before the fix it slipped through
    // because "need to read" without leading "i" wasn't in the list.
    let text = "Need to read the tool_loop_inner function entry to add turn timing. \
                Let me check that first.";
    assert!(
        has_phantom_tool_intent_no_tools(text),
        "the pronounless `Need to read` pattern must fire the phantom detector \
         — this was the exact leak shape reported by the user"
    );
}

#[test]
fn detects_pronounless_need_to_variants_individually() {
    // Spot-check each of the high-frequency verbs added. Each
    // sentence is the kind of opening line a model emits when it's
    // narrating instead of calling a tool.
    let leaks = [
        "Need to check the config first.",
        "Need to look at the error log.",
        "Need to examine the failing test.",
        "Need to verify the migration ran.",
        "Need to find where this is set.",
        "Need to investigate why the build broke.",
        "Need to update the version string.",
        "Need to fix the off-by-one.",
        "Need to write a test for this.",
        "Need to run cargo fmt.",
    ];
    for text in leaks {
        assert!(
            has_phantom_tool_intent_no_tools(text),
            "pronounless variant must fire for: {text:?}"
        );
    }
}

#[test]
fn pronounless_variants_dont_fire_on_unrelated_prose() {
    // "Need to X" is specific enough that natural code review prose
    // doesn't trigger it. These should NOT fire — the user's just
    // describing a situation, not narrating phantom intent.
    let safe = [
        // Note: the prose_lead_in stripper drops everything from the
        // first structural marker (table / list / code fence) onward,
        // so a code block containing "need to" is safe by structure.
        "The function works fine. No need to change anything.",
        "There's no need to worry about this case.",
    ];
    for text in safe {
        assert!(
            !has_phantom_tool_intent_no_tools(text),
            "prose without first-person narration intent must NOT fire: {text:?}"
        );
    }
}

#[test]
fn pre_existing_pronoun_prefixed_variants_still_fire() {
    // Make sure the "i need to X" entries still match — we added
    // the pronounless variants alongside, didn't remove the
    // existing ones.
    let text = "I need to read the source first.";
    assert!(has_phantom_tool_intent_no_tools(text));
}

#[test]
fn french_pronounless_besoin_de_fires() {
    // Telegraphic French — drops "j'ai" the same way English drops
    // "I". `j'ai besoin de` patterns existed; pronounless `besoin
    // de` did not until 2026-06-01.
    let text = "Besoin de vérifier le code avant de continuer.";
    assert!(
        has_phantom_tool_intent_no_tools(text),
        "French pronoun-dropped `besoin de X` must fire the detector"
    );
}

#[test]
fn detects_bare_gerund_user_exact_leak() {
    // Second leak reported 2026-06-01 — bare gerund sentence opener.
    // No "let me" / "I'll" / "now" anchor; the existing detector
    // missed it because the intent_phrases list required a pronoun
    // or imperative anchor.
    let text = "Reading the current state of the affected files to make precise edits.";
    assert!(
        has_phantom_tool_intent_no_tools(text),
        "the bare-gerund `Reading the current state ...` shape must fire \
         the phantom detector — this was the second leak reported by the user"
    );
}

#[test]
fn detects_bare_gerund_variants_individually() {
    let leaks = [
        "Checking the existing implementation before making changes.",
        "Examining the affected modules to plan the refactor.",
        "Inspecting the current configuration before editing.",
        "Verifying the existing logic before touching it.",
        "Reviewing the current behaviour to make sure we don't regress.",
        "Investigating the existing test coverage.",
        "Looking at the current state of the code.",
    ];
    for text in leaks {
        assert!(
            has_phantom_tool_intent_no_tools(text),
            "bare-gerund opener must fire for: {text:?}"
        );
    }
}

#[test]
fn russian_pronounless_nuzhno_still_fires() {
    // RU already had pronounless variants — guard against regression
    // if someone refactors the deferment block.
    let text = "Нужно прочитать исходник перед изменением.";
    assert!(
        has_phantom_tool_intent_no_tools(text),
        "Russian pronoun-dropped `нужно X` must continue to fire — \
         was present before this round of changes, keep it green"
    );
}
