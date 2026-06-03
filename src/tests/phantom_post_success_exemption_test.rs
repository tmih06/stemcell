//! Sentinel for the post-success phantom-tool-call exemption.
//!
//! Incident: the phantom detector treated text-only iterations the
//! same regardless of whether the turn had already produced
//! successful tool calls. After a clean commit+push the model would
//! emit a one-line "Pushed." or a short summary, the detector's
//! `has_past_tense_action_claim` would fire ("pushed" / "committed"
//! / "fixed" are all past-tense action verbs), `phantom_retries_used`
//! would tick up, and 5+ retries later self-heal would even swap
//! providers — all on completed work. User screenshots showed 8+
//! "Phantom tool calls detected" alerts and provider-swap warnings
//! after a successful commit, ~293s and 4683 tokens wasted finalising
//! nothing.
//!
//! Fix: a turn-scoped counter `tool_calls_completed_this_turn` ticks
//! every successful tool execution. The phantom detection block is
//! gated on `phantom_eligible = !is_cli_provider &&
//! tool_calls_completed_this_turn == 0`, so once the turn has real
//! work behind it the wrap-up text is treated as a completion ack,
//! not phantom intent.
//!
//! These tests are source-level sentinels:
//! 1. The counter is declared at turn scope (not iteration scope),
//!    initialised to zero, and incremented in both success paths
//!    (direct execution + post-approval execution).
//! 2. The four phantom-detection gates inside `tool_uses.is_empty()`
//!    all check `phantom_eligible`, not the bare `!is_cli_provider`
//!    that produced the false positive.
//! 3. The BRAIN_PREAMBLE "FINISHING A TURN" directive lands in the
//!    system prompt every turn, telling the model what shape the
//!    final acknowledgement should take. Without this the model
//!    still narrates after a successful tool call; the safe-guard
//!    catches the false positive, but the model wastes tokens on
//!    multi-paragraph wrap-ups before the loop ends.

const TOOL_LOOP_SRC: &str = include_str!("../brain/agent/service/tool_loop.rs");
const PROMPT_BUILDER_SRC: &str = include_str!("../brain/prompt_builder.rs");

#[test]
fn counter_declared_at_turn_scope_not_iteration_scope() {
    // The counter must be next to `let mut iteration = 0;` (turn-scoped),
    // NOT inside the iteration-body loop where `let mut tool_uses` lives
    // (iteration-scoped, resets every loop). Iteration-scoped would
    // defeat the whole point: every text-only iteration would see
    // counter == 0 again and the detector would re-fire.
    assert!(
        TOOL_LOOP_SRC.contains("let mut tool_calls_completed_this_turn: usize = 0;"),
        "tool_calls_completed_this_turn counter must be declared and zero-initialised"
    );
    // Position check: the counter must appear BEFORE `let mut iteration_text`
    // (which is iteration-scoped). If it ends up after, the iteration
    // body owns it and resets each loop.
    let counter_pos = TOOL_LOOP_SRC
        .find("let mut tool_calls_completed_this_turn")
        .expect("counter must exist");
    let iter_text_pos = TOOL_LOOP_SRC
        .find("let mut iteration_text = String::new();")
        .expect("iteration_text marker must exist");
    assert!(
        counter_pos < iter_text_pos,
        "counter must be declared OUTSIDE the iteration body (before `let mut iteration_text`) \
         or it resets every iteration and the exemption never works"
    );
}

#[test]
fn counter_increments_on_both_success_paths() {
    // Two distinct success branches exist (direct execution at
    // ~line 4041 and post-approval execution at ~line 3830). Both
    // must tick the counter or partial coverage lets the regression
    // come back through whichever branch was missed.
    let increments = TOOL_LOOP_SRC
        .matches("tool_calls_completed_this_turn += 1;")
        .count();
    assert!(
        increments >= 2,
        "counter must be incremented in BOTH the direct and post-approval success branches; \
         found {increments} increments — missing one re-opens the regression for that path"
    );
}

#[test]
fn phantom_eligible_gate_replaces_naked_is_cli_provider_check() {
    // The four phantom-detection gates inside `tool_uses.is_empty()`
    // must use the `phantom_eligible` gate (which folds in the
    // post-success exemption), not the bare `!is_cli_provider` that
    // ignored success state. Search for the unguarded pattern
    // inside a window around the phantom block.
    let anchor = "// ── Phantom tool call detection";
    let anchor_pos = TOOL_LOOP_SRC
        .find(anchor)
        .expect("phantom detection block marker must exist");
    let block_end = TOOL_LOOP_SRC[anchor_pos..]
        .find("// Cap hit and the fast-escalate block above couldn't")
        .map(|p| anchor_pos + p + 500)
        .unwrap_or(anchor_pos + 4000);
    let window = &TOOL_LOOP_SRC[anchor_pos..block_end.min(TOOL_LOOP_SRC.len())];
    // Whitespace-normalised match so rustfmt-driven line wrapping
    // doesn't false-fail (e.g. fmt may split the let-binding across
    // two lines — the chain shape is what matters).
    let normalized: String = window
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    // Two-regime gate (refined 2026-06-03 for forward-intent re-engagement
    // after a successful tool call). The original single-condition form
    // was correct until logs showed "Let me dig into …" leaking through
    // after one tool — see `forward_intent_after_tool_call_fires_phantom_via_dedicated_detector`.
    let expected_lead = "let phantom_eligible = !is_cli_provider";
    assert!(
        normalized.contains(expected_lead),
        "phantom_eligible gate must lead with `!is_cli_provider` — \
         expected substring: {expected_lead}"
    );
    let expected_zero_tools = "tool_calls_completed_this_turn == 0";
    assert!(
        normalized.contains(expected_zero_tools),
        "phantom_eligible gate must still include the zero-tools branch — \
         expected substring: {expected_zero_tools}"
    );
    let expected_forward_intent_call = "has_forward_intent_post_success";
    assert!(
        normalized.contains(expected_forward_intent_call),
        "phantom_eligible gate must call has_forward_intent_post_success so \
         forward-looking intent after a tool call re-engages self-heal — \
         expected substring: {expected_forward_intent_call}"
    );
    // The four phantom-detection conditions must check phantom_eligible.
    // Counting "phantom_eligible" usages inside the window gives us a
    // lower bound (the let-binding + at least 3 condition uses).
    let phantom_eligible_uses = window.matches("phantom_eligible").count();
    assert!(
        phantom_eligible_uses >= 4,
        "phantom_eligible must appear in the gate binding + the three condition checks; \
         found {phantom_eligible_uses} uses — a missing one leaves a phantom branch unguarded"
    );
}

#[test]
fn brain_preamble_directs_clear_acknowledgement_not_silent_close() {
    // Original directive wording ("ONE short acknowledgement line and
    // stop", with bare-word examples like "Done.") was interpreted by
    // the model as "produce no text at all" — it started emitting
    // `finish_reason: stop` with empty delta on basically every
    // side-effect turn. Looked like a silent crash to the user.
    //
    // The rewritten directive REQUIRES the acknowledgement and tells
    // the model that empty completions are the worst possible outcome.
    // Pin the key phrases that prevent the regression from sneaking
    // back via a refactor.
    assert!(
        PROMPT_BUILDER_SRC.contains("FINISHING A TURN"),
        "BRAIN_PREAMBLE must carry the FINISHING A TURN directive header"
    );
    assert!(
        PROMPT_BUILDER_SRC.contains("never disappear silently"),
        "directive header must forbid empty closes — the bug it was added to prevent"
    );
    assert!(
        PROMPT_BUILDER_SRC.contains("never end with empty content"),
        "directive must explicitly forbid `finish_reason: stop` with no text"
    );
    assert!(
        PROMPT_BUILDER_SRC.contains("Empty completions"),
        "directive must call out empty completions as a failure mode, not a valid close"
    );
    assert!(
        PROMPT_BUILDER_SRC.contains("Do NOT run \"verification\" tool calls"),
        "directive must forbid verification re-runs — the secondary loop pattern"
    );
}

#[test]
fn comment_documents_the_post_success_exemption_rationale() {
    // The exemption is non-obvious and a refactor could remove it
    // without realising what it does. The inline comment must
    // explain the rationale so the next person editing this block
    // doesn't strip the gate "for cleanliness" and re-introduce
    // the loop.
    assert!(
        TOOL_LOOP_SRC.contains("POST-SUCCESS EXEMPTION"),
        "the gate must carry a labelled comment so its purpose survives refactors"
    );
    assert!(
        TOOL_LOOP_SRC.contains("completion acknowledgement"),
        "comment must name what the text-only iteration actually is"
    );
}

// === Forward-intent re-engagement after a successful tool call ===
//
// 2026-06-03 regression: a turn ran one `bash: git branch
// --show-current` tool call then emitted prose with three FORWARD-
// looking intent phrases:
//
//   "Good, on main. Let me dig into the delete invitation endpoint,
//    the email send path, and the invite flow to find the bugs."
//
// No further tool call followed. The original exemption disabled
// phantom for the whole post-tool portion of the turn, so the three
// promised investigations silently dropped. The refined gate flips
// `phantom_eligible` back ON when the iteration text carries a
// forward-looking intent phrase, while keeping pure completion acks
// (`Pushed.` / `Committed.` / `On main.`) exempt as before.

use crate::brain::agent::service::{has_forward_intent_post_success, has_phantom_tool_intent_no_tools};

#[test]
fn forward_intent_after_tool_call_fires_phantom_via_dedicated_detector() {
    // The exact text from the 2026-06-03 screenshot. Must trigger
    // the post-success forward-intent detector even though the turn
    // had already produced a successful tool call.
    let text = "Good, on main. Let me dig into the delete invitation endpoint, the email \
                send path, and the invite flow to find the bugs.";
    assert!(
        has_forward_intent_post_success(text),
        "the exact leak text from 2026-06-03 must fire the post-success \
         forward-intent detector — three promised investigations dropped \
         silently because the previous exemption gated phantom on tools-\
         completed == 0 and disabled the check entirely after the first tool"
    );
}

#[test]
fn pure_completion_ack_after_tool_does_not_fire_forward_intent_detector() {
    // The whole point of the original exemption (e843f405): once a
    // tool call has succeeded, the model's wrap-up text is a
    // legitimate completion ack and must NOT trigger phantom. The
    // forward-intent variant skips the past-tense branch entirely
    // so these ack shapes pass through clean.
    for ack in [
        "Pushed.",
        "Committed.",
        "Done. On main.",
        "Pushed and tagged. All green on CI.",
        "Migration created. Files saved.",
        "Updated the file. Bumped the version.",
    ] {
        assert!(
            !has_forward_intent_post_success(ack),
            "pure completion ack must NOT fire post-success forward-intent \
             detector — that's what the original exemption protects against. \
             Got a hit on: {ack:?}"
        );
    }
}

#[test]
fn presentation_verbs_after_tool_do_not_fire_forward_intent_detector() {
    // Common post-completion phrasings that LOOK like "let me X" but
    // are presentation/communication actions, not pre-tool narration.
    // The curated intent_phrases list deliberately excludes these
    // verbs (`know`, `show`, `explain`, `tell`); the dual-check via
    // KNOWN_TOOL_NAMES protects against false positives here.
    for safe in [
        "All done. Let me know if you need anything else.",
        "Pushed. Let me show you the diff in a sec.",
        "Committed. I'll explain the structure if useful.",
        "Tag added. Let me tell you what changed since the last release.",
    ] {
        assert!(
            !has_forward_intent_post_success(safe),
            "presentation-verb phrasing must NOT fire the post-success \
             forward-intent detector. Got a hit on: {safe:?}"
        );
    }
}

#[test]
fn forward_intent_detector_uses_prose_lead_in_filter() {
    // Structural content past the lead-in (tables, code blocks,
    // bullet lists, commit message bodies) must NOT contribute to
    // matches. This mirrors `has_phantom_tool_intent_no_tools` and
    // protects against the e843f405 original false positive where
    // commit messages containing "Let me X" inside their body
    // re-triggered the detector after a successful push.
    let text = "All set. Pushed and tagged.\n\
                \n\
                | step | result |\n\
                | --- | --- |\n\
                | Let me check the build | passed |\n\
                | Let me dig into logs | clean |\n";
    assert!(
        !has_forward_intent_post_success(text),
        "the prose lead-in here is just 'All set. Pushed and tagged.' — \
         the structural table past it must NOT contribute matches even \
         though it contains literal 'let me check' / 'let me dig' phrases"
    );
}

#[test]
fn standard_phantom_detector_still_fires_on_zero_tool_case() {
    // Sanity: the broader `has_phantom_tool_intent_no_tools` (used by
    // the zero-tool-call regime) still catches the same forward
    // intent it always did. The new function is a sister to that
    // one, not a replacement.
    let text = "Good, on main. Let me dig into the delete invitation endpoint.";
    assert!(has_phantom_tool_intent_no_tools(text));
    assert!(has_forward_intent_post_success(text));
}

#[test]
fn eligibility_gate_in_tool_loop_uses_forward_intent_path_after_first_tool() {
    // Source-level guard: the eligibility gate must include the
    // forward-intent detector inside its disjunction so a future
    // refactor that "simplifies" the gate back to the original
    // `tools_completed == 0` form fails this test rather than
    // silently re-opening the 2026-06-03 drop.
    assert!(
        TOOL_LOOP_SRC.contains("has_forward_intent_post_success"),
        "the phantom-eligibility gate in run_tool_loop_inner must call \
         has_forward_intent_post_success so forward-looking intent after \
         a successful tool call re-engages self-heal"
    );
}
