//! Tests for plan-tool summary rendering in `markdown_to_telegram_html`.
//!
//! The `plan` tool's `summary` op emits a block like:
//!
//!     📊 Plan Summary
//!
//!     Plan: My Plan
//!     Status: InProgress
//!     ...
//!     Tasks (3 total):
//!       ✅ 1. First task
//!       ▶️ 2. Second task
//!       ⏸️ 3. Third task
//!
//!     Progress: 33.3% — ✅1 ❌0 ▶️1 ⏸️1 ⏭️0 🚫0
//!     Success Rate: 100.0% | Retries: 0 | Tool Calls: 0
//!
//! Telegram's HTML mode strips leading whitespace and collapses the
//! task indents into one busy line. The converter now wraps the
//! whole block in `<pre>...</pre>` so the indent and emoji icons
//! render as a monospace status panel.

use crate::channels::telegram::handler::markdown_to_telegram_html;

const PLAN_BLOCK: &str = "📊 Plan Summary\n\n\
                          Plan: My Plan\n\
                          Status: InProgress\n\
                          Description: refactor X\n\n\
                          Tasks (3 total):\n  \
                          ✅ 1. First task\n  \
                          ▶️ 2. Second task\n  \
                          ⏸️ 3. Third task\n\n\
                          Progress: 33.3% — ✅1 ❌0 ▶️1 ⏸️1 ⏭️0 🚫0\n\
                          Success Rate: 100.0% | Retries: 0 | Tool Calls: 0";

#[test]
fn plan_block_is_wrapped_in_pre() {
    let html = markdown_to_telegram_html(PLAN_BLOCK);
    assert!(
        html.contains("<pre>📊 Plan Summary"),
        "plan block must open with <pre>; got: {html}"
    );
    assert!(
        html.contains("Success Rate: 100.0% | Retries: 0 | Tool Calls: 0\n</pre>")
            || html.contains("Success Rate: 100.0% | Retries: 0 | Tool Calls: 0</pre>"),
        "plan block must close </pre> after the Success Rate line; got: {html}"
    );
}

#[test]
fn plan_block_preserves_task_indent() {
    let html = markdown_to_telegram_html(PLAN_BLOCK);
    // Inside <pre> the 2-space indent must survive escaping. The
    // <pre> tag means Telegram renders whitespace literally.
    assert!(
        html.contains("  ✅ 1. First task"),
        "task indent and emoji must survive; got: {html}"
    );
    assert!(
        html.contains("  ▶️ 2. Second task"),
        "second task indent must survive; got: {html}"
    );
    assert!(
        html.contains("  ⏸️ 3. Third task"),
        "third task indent must survive; got: {html}"
    );
}

#[test]
fn plan_block_keeps_progress_line_intact() {
    let html = markdown_to_telegram_html(PLAN_BLOCK);
    assert!(
        html.contains("Progress: 33.3% — ✅1 ❌0 ▶️1 ⏸️1 ⏭️0 🚫0"),
        "progress line must render as a single line inside the panel; got: {html}"
    );
}

#[test]
fn text_before_and_after_plan_block_processes_normally() {
    // The agent often emits text around a plan summary — confirm
    // markdown still gets converted for that text and the plan
    // block doesn't bleed into surrounding content.
    let mixed = format!(
        "I've made progress on the refactor. Here's where we are:\n\n\
         {PLAN_BLOCK}\n\n\
         Next step: implement the second task."
    );
    let html = markdown_to_telegram_html(&mixed);
    assert!(
        html.contains("I&#x27;ve made progress on the refactor")
            || html.contains("I've made progress on the refactor"),
        "leading prose must render normally; got: {html}"
    );
    assert!(
        html.contains("Next step: implement the second task"),
        "trailing prose must render normally; got: {html}"
    );
    assert!(
        html.contains("<pre>📊 Plan Summary"),
        "plan block still wrapped; got: {html}"
    );
    // The trailing prose must be OUTSIDE the pre block.
    let close_pre = html.find("</pre>").expect("must have a closing pre tag");
    let next_step = html.find("Next step").expect("must contain trailing prose");
    assert!(
        next_step > close_pre,
        "trailing prose must come after </pre>, not be wrapped inside; got: {html}"
    );
}

#[test]
fn truncated_plan_block_still_closes_pre() {
    // Streaming may cut off mid-summary before the Success Rate
    // footer. The converter must still close the <pre> at end of
    // input so Telegram doesn't reject the HTML.
    let truncated = "📊 Plan Summary\n\nPlan: Half-streamed\nStatus: InProgress";
    let html = markdown_to_telegram_html(truncated);
    let opens = html.matches("<pre>").count();
    let closes = html.matches("</pre>").count();
    assert_eq!(
        opens, closes,
        "open/close <pre> count must balance even for truncated stream; got: {html}"
    );
}

#[test]
fn message_without_plan_block_is_unchanged() {
    let plain = "Just a regular reply with **bold** and `code`.";
    let html = markdown_to_telegram_html(plain);
    assert!(
        !html.contains("<pre>"),
        "non-plan messages must not gain spurious <pre> wrapping; got: {html}"
    );
}

#[test]
fn plan_block_with_no_trailing_text_still_closes() {
    let html = markdown_to_telegram_html(PLAN_BLOCK);
    let opens = html.matches("<pre>").count();
    let closes = html.matches("</pre>").count();
    assert_eq!(opens, 1, "expected one <pre>; got: {html}");
    assert_eq!(closes, 1, "expected one </pre>; got: {html}");
}

#[test]
fn html_chars_inside_plan_are_escaped() {
    // A task title containing `<` or `>` must be escaped inside
    // <pre>, otherwise Telegram rejects the message as malformed.
    let plan = "📊 Plan Summary\n\n\
                Tasks (1 total):\n  \
                ✅ 1. Fix <legacy> handler\n\n\
                Progress: 100% — ✅1 ❌0 ▶️0 ⏸️0 ⏭️0 🚫0\n\
                Success Rate: 100.0% | Retries: 0 | Tool Calls: 0";
    let html = markdown_to_telegram_html(plan);
    assert!(
        html.contains("&lt;legacy&gt;"),
        "angle brackets must be escaped; got: {html}"
    );
    assert!(
        !html.contains("<legacy>"),
        "raw angle brackets must NOT leak; got: {html}"
    );
}

#[test]
fn two_plan_blocks_in_one_message_both_wrap() {
    // The agent could emit two summary updates in one turn (e.g.
    // "Here's where we were, here's where we are now"). Each must
    // get its own <pre> box.
    let two = format!("{PLAN_BLOCK}\n\nAnd now:\n\n{PLAN_BLOCK}");
    let html = markdown_to_telegram_html(&two);
    assert_eq!(html.matches("<pre>").count(), 2, "got: {html}");
    assert_eq!(html.matches("</pre>").count(), 2, "got: {html}");
}
