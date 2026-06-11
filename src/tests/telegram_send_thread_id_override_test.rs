//! Tests for `telegram_send::resolve_thread_id` — the helper that
//! lets the agent pass an explicit `thread_id` to override the
//! auto-lookup behavior added in commit `1e46cd26` (closes #130).
//!
//! Use cases the override enables:
//!   * Cron jobs posting to a specific topic (e.g. daily summary in
//!     #announcements regardless of what was discussed last in #dev).
//!   * Multi-topic conversations where the agent wants to post into
//!     a topic OTHER than the most recent one stored.
//!   * Explicit user instruction: "post this in topic 17".
//!
//! Suggested by leshchenko1979 on issue #130:
//! https://github.com/tmih06/stemcell/issues/130#issuecomment-4582189795

use crate::channels::telegram::send::resolve_thread_id;
use serde_json::json;
use teloxide::types::{MessageId, ThreadId};

#[tokio::test]
async fn explicit_thread_id_is_returned_verbatim() {
    let input = json!({ "thread_id": 17 });
    let result = resolve_thread_id(&input, 0).await;
    assert_eq!(result, Some(ThreadId(MessageId(17))));
}

#[tokio::test]
async fn explicit_thread_id_works_for_negative_legacy_thread_ids() {
    // Legacy Telegram chat shapes occasionally surface negative
    // thread_id values within i32 range. The helper must pass them
    // through, not reject them.
    let input = json!({ "thread_id": -2147483 });
    let result = resolve_thread_id(&input, 0).await;
    assert_eq!(result, Some(ThreadId(MessageId(-2147483))));
}

#[tokio::test]
async fn explicit_thread_id_overflowing_i32_falls_back_to_lookup() {
    // teloxide's ThreadId wraps MessageId(i32). Values past i32::MAX
    // can't be represented. Rather than returning a wrong/zero ID,
    // the helper falls through to the auto-lookup path.
    let input = json!({ "thread_id": 9_999_999_999_i64 });
    // No global pool initialised → auto-lookup returns None → final
    // result is None. We just confirm no panic + no garbage value.
    let result = resolve_thread_id(&input, 12345).await;
    assert_eq!(result, None);
}

#[tokio::test]
async fn no_explicit_thread_id_falls_back_to_lookup() {
    // Absent field → auto-lookup path. In tests the global pool
    // isn't initialised so the lookup returns None; the important
    // contract is "no override path, no garbage value, no panic".
    let input = json!({ "chat_id": 12345 });
    let result = resolve_thread_id(&input, 12345).await;
    assert_eq!(result, None);
}

#[tokio::test]
async fn non_integer_thread_id_falls_back_to_lookup() {
    // Defensive: the agent could emit "thread_id": "17" (string) or
    // an object/array. Don't accept those as overrides — fall back
    // to auto-lookup so a malformed override doesn't poison routing.
    let input = json!({ "thread_id": "17" });
    let result = resolve_thread_id(&input, 12345).await;
    // Auto-lookup returns None in test (no global pool).
    assert_eq!(result, None);
}

#[tokio::test]
async fn explicit_thread_id_zero_is_returned() {
    // Telegram's General topic is sometimes represented as thread 0
    // depending on API surface. The helper doesn't second-guess
    // i32-valid values.
    let input = json!({ "thread_id": 0 });
    let result = resolve_thread_id(&input, 0).await;
    assert_eq!(result, Some(ThreadId(MessageId(0))));
}
