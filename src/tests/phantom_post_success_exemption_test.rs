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
    let expected_chain =
        "let phantom_eligible = !is_cli_provider && tool_calls_completed_this_turn == 0;";
    assert!(
        normalized.contains(expected_chain),
        "phantom_eligible gate must be defined at the top of the phantom block — \
         expected substring: {expected_chain}"
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
fn brain_preamble_directs_one_line_completion() {
    // The model needs to KNOW the expected shape of a completion
    // turn, not just have the runtime catch the loop. Pin the
    // directive's key phrases so a future preamble refactor doesn't
    // accidentally drop it.
    assert!(
        PROMPT_BUILDER_SRC.contains("FINISHING A TURN"),
        "BRAIN_PREAMBLE must carry the FINISHING A TURN directive header"
    );
    assert!(
        PROMPT_BUILDER_SRC.contains("ONE short acknowledgement line"),
        "directive must spell out the one-line shape"
    );
    assert!(
        PROMPT_BUILDER_SRC.contains("Do NOT run \"verification\" tool calls"),
        "directive must forbid verification re-runs — the secondary loop pattern"
    );
    assert!(
        PROMPT_BUILDER_SRC.contains("you are looping on a completed task"),
        "directive must give the model a self-detection signal so it can break out \
         even when the runtime exemption hasn't caught up yet"
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
