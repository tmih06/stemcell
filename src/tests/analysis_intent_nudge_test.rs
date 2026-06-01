//! Sentinel tests for the `is_analysis_intent` helper + the
//! empty-analysis nudge that uses it.
//!
//! Regression context: commit e843f405 added a "FINISHING A TURN"
//! directive to BRAIN_PREAMBLE telling the model to end with a
//! one-line "Done." after successful tool calls. That worked for
//! side-effect tasks (commit / push / edit) but overshot into
//! data-analysis tasks ("audit the PR", "compare A and B",
//! "explain Z") where the tool fetches data the user expects
//! summarised. Result: the model emitted `finish_reason: stop`
//! with empty delta after `gh pr view`, leaving the user with
//! zero text answer and 7 visible tool calls.
//!
//! Fix (this file pins the two halves):
//!
//! 1. `is_analysis_intent` recognises analysis-shaped user
//!    requests at a word boundary. Matched verbs trigger the
//!    runtime nudge when a turn ends empty after tool calls.
//! 2. The `FINISHING A TURN` directive in BRAIN_PREAMBLE now
//!    splits into two cases: side-effect tasks get a one-liner,
//!    analysis tasks must produce real text answers using the
//!    fetched data.
//! 3. Source-level sentinel asserts the nudge wiring in
//!    `run_tool_loop_inner` continues to gate on
//!    `is_analysis_intent` + empty iteration_text + tool-success
//!    so the regression mode can't sneak back through a refactor.

use crate::brain::agent::service::is_analysis_intent;

const PROMPT_BUILDER_SRC: &str = include_str!("../brain/prompt_builder.rs");
const TOOL_LOOP_SRC: &str = include_str!("../brain/agent/service/tool_loop.rs");

#[test]
fn detects_audit_verb_in_user_request() {
    assert!(is_analysis_intent("audit the PR for me"));
    assert!(is_analysis_intent("Audit the PR"));
    assert!(is_analysis_intent("can you audit the latest PRs"));
}

#[test]
fn detects_review_compare_explain_at_start() {
    assert!(is_analysis_intent("review this PR"));
    assert!(is_analysis_intent("compare these two changes"));
    assert!(is_analysis_intent("explain what this code does"));
    assert!(is_analysis_intent("summarise the changes since v0.3.31"));
    assert!(is_analysis_intent("summarize the failures from yesterday"));
}

#[test]
fn detects_question_shapes() {
    assert!(is_analysis_intent("what does this function do"));
    assert!(is_analysis_intent("how does the fallback chain work"));
    assert!(is_analysis_intent("why does the agent loop on completion"));
    assert!(is_analysis_intent("what is the current model"));
    assert!(is_analysis_intent("what are the open issues"));
}

#[test]
fn detects_show_tell_investigate_diagnose() {
    assert!(is_analysis_intent("tell me what changed"));
    assert!(is_analysis_intent("show me the recent commits"));
    assert!(is_analysis_intent("investigate the phantom loop"));
    assert!(is_analysis_intent("diagnose the crash"));
}

#[test]
fn detects_check_describe_find() {
    assert!(is_analysis_intent("check the logs"));
    assert!(is_analysis_intent("describe the schema"));
    assert!(is_analysis_intent("find the bug in this function"));
    assert!(is_analysis_intent("look up the docs for X"));
    assert!(is_analysis_intent("look at issue #141"));
}

#[test]
fn does_not_match_side_effect_verbs() {
    // Commit / push / edit / delete are side-effect tasks where
    // the tool result IS the deliverable. The nudge must NOT
    // fire on them or we'd loop on legitimate "Done." closes.
    assert!(!is_analysis_intent("commit the changes"));
    assert!(!is_analysis_intent("push to main"));
    assert!(!is_analysis_intent("edit the file to fix the bug"));
    assert!(!is_analysis_intent("delete the old config"));
    assert!(!is_analysis_intent("close issue #141"));
    assert!(!is_analysis_intent("tag the release"));
    assert!(!is_analysis_intent("create a PR"));
    assert!(!is_analysis_intent("send a message to slack"));
}

#[test]
fn does_not_trip_on_verb_inside_other_words() {
    // The leading-word match guards against substring false
    // positives. "auditorium" must not match "audit", "examination"
    // must not match "exam", "describes" inside a quote must not
    // false-positive.
    assert!(!is_analysis_intent("the auditorium was packed"));
    assert!(!is_analysis_intent("examines the wreckage"));
    assert!(!is_analysis_intent("the report says nothing"));
    // "find" at the START would match, so we test a substring case
    // where it shouldn't.
    assert!(!is_analysis_intent("we couldn't refind the missing key"));
}

#[test]
fn handles_channel_prefix_wrapped_messages() {
    // Telegram / Discord wrap incoming messages in a `[Channel: ...]\n<msg>`
    // prefix. The helper looks at the last line so the wrapper
    // doesn't shadow the actual request.
    let wrapped = "[Channel: Telegram — your text response is automatically sent. \
                   Do NOT call telegram_send to deliver your answer. Only use \
                   telegram_send for: sending to a different chat_id, media, polls.]\n\
                   audit the latest PR";
    assert!(
        is_analysis_intent(wrapped),
        "must detect the analysis verb on the line AFTER the channel prefix"
    );
}

#[test]
fn empty_or_whitespace_input_returns_false() {
    assert!(!is_analysis_intent(""));
    assert!(!is_analysis_intent("   "));
    assert!(!is_analysis_intent("\n\n"));
}

// ── Directive + runtime wiring sentinels ────────────────────────

#[test]
fn brain_preamble_splits_finishing_a_turn_into_two_cases() {
    // The directive must explicitly carry BOTH the side-effect
    // and data-analysis branches. Earlier wording (pre-fix) only
    // had the side-effect branch and produced the empty-analysis
    // regression.
    assert!(
        PROMPT_BUILDER_SRC.contains("SIDE-EFFECT tasks"),
        "directive must label the side-effect branch"
    );
    assert!(
        PROMPT_BUILDER_SRC.contains("DATA-FETCH / ANALYSIS tasks"),
        "directive must label the analysis branch — the gap that produced \
         the empty-text-after-tool-calls regression"
    );
    assert!(
        PROMPT_BUILDER_SRC.contains("INPUT to your answer"),
        "directive must spell out that for analysis tasks the fetched data \
         is INPUT, not the answer itself"
    );
    assert!(
        PROMPT_BUILDER_SRC.contains("\"Done.\" after `gh pr view` is WRONG"),
        "directive must call out the specific failure shape from the user incident"
    );
}

#[test]
fn tool_loop_wires_analysis_nudge_at_turn_end_site() {
    // The nudge MUST be gated on (a) empty iteration_text,
    // (b) tool_calls_completed_this_turn > 0, AND
    // (c) is_analysis_intent on the user message. Missing any
    // gate re-opens the regression for that path.
    assert!(
        TOOL_LOOP_SRC.contains("let mut analysis_nudge_used: bool = false;"),
        "one-shot nudge budget must be declared at turn scope"
    );
    assert!(
        TOOL_LOOP_SRC.contains("super::phantom::is_analysis_intent(user_text_for_intent)"),
        "nudge condition must call is_analysis_intent on the clean user text \
         (display_text_override-aware so channel prefixes don't shadow the verb)"
    );
    assert!(
        TOOL_LOOP_SRC.contains("iteration_text.trim().is_empty()"),
        "nudge must fire when the final iteration produced zero text — \
         that's the empty-close case"
    );
    assert!(
        TOOL_LOOP_SRC.contains("tool_calls_completed_this_turn > 0"),
        "nudge must require at least one successful tool call this turn \
         — otherwise this isn't the empty-after-fetch case, it's just \
         a model that refused to answer at all"
    );
}
