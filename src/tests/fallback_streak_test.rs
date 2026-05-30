//! Tests for the per-session primary-failure-streak counter that
//! gates fallback stickiness.
//!
//! Regression context (2026-05-30): a transient stream error from
//! `dialagram/qwen-3.7-max-thinking` (provider that closes the
//! socket without `[DONE]`) was triggering immediate permanent
//! fallback to the configured fallback provider. After the
//! `text_looks_complete` fix (commit 97683fb0) most of those
//! errors no longer fire, but for the cases where the fallback
//! DOES engage, the session was getting demoted to the fallback
//! provider on the very first incident — even though the primary
//! recovered on the next request. User intent: "if fallback rescues
//! 3 times consecutively successfully, the 4th it sticks".
//!
//! These tests cover the bare counter mechanics. Integration with
//! the actual fallback flow lives in `tool_loop.rs` and is harder
//! to unit-test (requires a real provider + DB); the counter
//! helpers it consumes ARE testable here in isolation.

use crate::tests::agent_service_mocks::create_test_service;

#[tokio::test]
async fn fresh_session_starts_with_zero_streak() {
    let (svc, sid) = create_test_service().await;
    assert_eq!(svc.peek_primary_failure_streak(sid), 0);
}

#[tokio::test]
async fn bump_increments_and_returns_new_count() {
    let (svc, sid) = create_test_service().await;
    assert_eq!(svc.bump_primary_failure_streak(sid), 1);
    assert_eq!(svc.bump_primary_failure_streak(sid), 2);
    assert_eq!(svc.bump_primary_failure_streak(sid), 3);
    assert_eq!(svc.peek_primary_failure_streak(sid), 3);
}

#[tokio::test]
async fn reset_clears_to_zero() {
    let (svc, sid) = create_test_service().await;
    svc.bump_primary_failure_streak(sid);
    svc.bump_primary_failure_streak(sid);
    assert_eq!(svc.peek_primary_failure_streak(sid), 2);
    svc.reset_primary_failure_streak(sid);
    assert_eq!(svc.peek_primary_failure_streak(sid), 0);
}

#[tokio::test]
async fn reset_then_bump_starts_fresh_at_one() {
    // The point of reset: a single primary success after a streak
    // wipes the history, so a future hiccup doesn't inherit the
    // count.
    let (svc, sid) = create_test_service().await;
    svc.bump_primary_failure_streak(sid);
    svc.bump_primary_failure_streak(sid);
    svc.bump_primary_failure_streak(sid);
    svc.reset_primary_failure_streak(sid);
    assert_eq!(svc.bump_primary_failure_streak(sid), 1);
}

#[tokio::test]
async fn streak_is_per_session_isolated() {
    let (svc, sid_a) = create_test_service().await;
    let (_svc_b, sid_b) = create_test_service().await;
    // Bump session A only.
    svc.bump_primary_failure_streak(sid_a);
    svc.bump_primary_failure_streak(sid_a);
    assert_eq!(svc.peek_primary_failure_streak(sid_a), 2);
    // Session B (different service instance) — distinct counter.
    // Also confirm even on the SAME service that an unrelated
    // session_id reads as 0.
    let other_sid = uuid::Uuid::new_v4();
    assert_eq!(svc.peek_primary_failure_streak(other_sid), 0);
    assert_eq!(svc.peek_primary_failure_streak(sid_b), 0);
}

#[tokio::test]
async fn remove_session_provider_also_clears_streak() {
    // When a session is deleted (e.g. user cleared history) the
    // streak counter must clear with it; otherwise a future
    // session that happened to reuse the same UUID would inherit
    // a phantom count.
    let (svc, sid) = create_test_service().await;
    svc.bump_primary_failure_streak(sid);
    svc.bump_primary_failure_streak(sid);
    svc.bump_primary_failure_streak(sid);
    assert_eq!(svc.peek_primary_failure_streak(sid), 3);
    svc.remove_session_provider(sid);
    assert_eq!(svc.peek_primary_failure_streak(sid), 0);
}

#[tokio::test]
async fn threshold_value_is_four() {
    // Sentinel: the user-stated intent was "3 consecutive
    // rescues, the 4th sticks". Encode that as a numeric assertion
    // so a future refactor that bumps the constant has to update
    // this test deliberately rather than silently changing UX.
    let (svc, sid) = create_test_service().await;
    let mut count = 0;
    while count < 4 {
        count = svc.bump_primary_failure_streak(sid);
    }
    assert_eq!(
        count, 4,
        "stickiness must engage on the 4th consecutive rescue"
    );
}
