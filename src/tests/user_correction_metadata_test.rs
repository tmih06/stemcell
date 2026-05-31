//! Sentinel for the `user_correction` feedback-metadata fix
//! (PR #140 by @leshchenko1979, closing #138).
//!
//! Bug context: when an agent input comes in through a channel
//! handler (Telegram / Discord / Slack / WhatsApp / Trello), the
//! handler wraps it in a `[Channel: <name> — ...]\n<actual>`
//! prefix so the model knows the delivery surface. The Telegram
//! prefix alone is 236 chars. The auto-recorded `user_correction`
//! metadata at `tool_loop.rs` used `user_message.chars().take(200)`
//! which captured only the truncated prefix — not a single byte of
//! the user's actual correction text survived into the feedback
//! ledger. RSI analysis of corrections could not see what users
//! were correcting.
//!
//! Fix: prefer `display_text_override` (the clean user message
//! that channels already pass through `send_message_with_tools_and_display`
//! for exactly this kind of "store the human text, not the wrapper"
//! purpose) and fall back to `user_message` for TUI / CLI sessions
//! where there is no wrapper.
//!
//! Why a source-level sentinel rather than a behaviour test: the
//! buggy line is inside `run_tool_loop_inner` and writes to a
//! sqlite-backed `feedback_ledger`. A real behaviour test needs
//! an AgentService + ChannelMessageRepository + DB pool — overkill
//! for a one-line fix. Pinning the actual source text catches the
//! exact regression mode (someone reverts to `user_message.chars()`
//! during a refactor) at compile time, which is what we care about.

const TOOL_LOOP_SRC: &str = include_str!("../brain/agent/service/tool_loop.rs");

/// Collapse every run of ASCII whitespace (spaces, tabs, newlines) to
/// a single space. Lets the sentinel match the chain whether rustfmt
/// keeps it on one line or fans it out across several. Avoids the
/// regression mode where someone runs `cargo fmt`, the multi-line
/// reformat survives, the chain shape is still correct, but a naive
/// `contains("display_text_override.as_deref()...")` check fails on
/// whitespace alone.
fn normalize_whitespace(s: &str) -> String {
    s.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn user_correction_metadata_uses_display_text_override() {
    // The fix prefers display_text_override and falls back to
    // user_message. Pin the chain shape on a whitespace-normalised
    // copy of the source so rustfmt-driven multi-line reformats
    // don't false-fail this regression check — what we care about
    // is the call order, not the indentation.
    let normalized = normalize_whitespace(TOOL_LOOP_SRC);
    let snippet = "display_text_override .as_deref() .unwrap_or(&user_message) .chars() .take(200)";
    let snippet_norm = normalize_whitespace(snippet);
    assert!(
        normalized.contains(&snippet_norm),
        "the user_correction metadata path must prefer display_text_override \
         over user_message. If you reverted to `user_message.chars().take(200)` \
         the Telegram / Discord / Slack / WhatsApp / Trello prefix (236+ chars \
         on Telegram) would consume the whole 200-char window and the actual \
         user correction text would never reach the feedback ledger. \
         Expected (whitespace-normalised) substring: {snippet_norm}"
    );
}

#[test]
fn no_naive_user_message_chars_take_200_in_user_correction_block() {
    // Negative assertion guarding against partial reverts where
    // someone keeps the display_text_override import but switches
    // the actual call back to the bug shape. The naive pattern is
    // a regression marker; if it ever appears in the
    // `user_correction` block again we want the test to scream.
    //
    // We scope the search to a window around the user_correction
    // call so the assertion doesn't false-positive on unrelated
    // 200-char-truncations elsewhere in the file.
    let anchor = "\"user_correction\",";
    let anchor_pos = TOOL_LOOP_SRC
        .find(anchor)
        .expect("the user_correction recording block must still exist in tool_loop.rs");
    let window_end = (anchor_pos + 400).min(TOOL_LOOP_SRC.len());
    let window = &TOOL_LOOP_SRC[anchor_pos..window_end];
    assert!(
        !window.contains("user_message.chars().take(200)"),
        "user_correction block contains the naive `user_message.chars().take(200)` \
         again — this is the #138 regression. Use \
         `display_text_override.as_deref().unwrap_or(&user_message).chars().take(200)` \
         instead so channel prefixes don't eat the metadata window."
    );
}

#[test]
fn telegram_channel_prefix_is_long_enough_to_eat_the_window() {
    // Lock in the assumption the fix is built on: the Telegram
    // wrapper prefix is longer than the 200-char metadata window.
    // If anyone shortens the prefix below 200 chars in the future
    // the original bug becomes invisible (truncation would
    // accidentally include some user text). This test is a
    // documentation anchor — it stays green even after a shrink,
    // but it documents WHY the fix matters by asserting the
    // current size.
    const TELEGRAM_PREFIX: &str = "[Channel: Telegram — your text response is automatically sent to this chat. \
         Do NOT call telegram_send to deliver your answer. Only use telegram_send for: \
         sending to a different chat_id, media, polls, buttons, reactions, or moderation.]\n";
    let char_count = TELEGRAM_PREFIX.chars().count();
    assert!(
        char_count > 200,
        "the Telegram channel prefix is {char_count} chars — the 200-char metadata \
         window is the exact reason #138 happened. If this prefix is now shorter \
         than 200 chars, the original bug shape changes (truncation might include \
         some real user text by accident) and this comment is the place to update \
         that history."
    );
}
