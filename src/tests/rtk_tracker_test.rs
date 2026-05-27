//! Tests for `src/rtk/tracker.rs` — moved out of an inline `#[cfg(test)]
//! mod tests` block so the project keeps every test under `src/tests/`.
//!
//! Tracker methods are async because the underlying `tokio::sync::Mutex`
//! requires it. Sync `std::sync::Mutex` inside an async path is the same
//! class of bug that hung the daemon in issue #125, so the lock primitive
//! was switched out for defense in depth.

use crate::rtk::{RtkTracker, TokenSavings};
use chrono::Utc;

#[tokio::test]
async fn test_tracker_creation() {
    let tracker = RtkTracker::new();
    assert_eq!(tracker.total_commands().await, 0);
    assert_eq!(tracker.total_tokens_saved().await, 0);
}

#[tokio::test]
async fn test_record_savings() {
    let tracker = RtkTracker::new();

    let savings = TokenSavings {
        command: "git status".to_string(),
        rewritten_command: "rtk git status".to_string(),
        original_tokens: 100,
        filtered_tokens: 20,
        tokens_saved: 80,
        savings_percent: 80.0,
        timestamp: Utc::now(),
    };

    tracker.record_savings(savings).await;

    assert_eq!(tracker.total_commands().await, 1);
    assert_eq!(tracker.total_tokens_saved().await, 80);
    assert!((tracker.average_savings_percent().await - 80.0).abs() < 0.01);
}

#[tokio::test]
async fn test_multiple_commands() {
    let tracker = RtkTracker::new();

    tracker
        .record_savings(TokenSavings {
            command: "git status".to_string(),
            rewritten_command: "rtk git status".to_string(),
            original_tokens: 100,
            filtered_tokens: 20,
            tokens_saved: 80,
            savings_percent: 80.0,
            timestamp: Utc::now(),
        })
        .await;

    tracker
        .record_savings(TokenSavings {
            command: "cargo build".to_string(),
            rewritten_command: "rtk cargo build".to_string(),
            original_tokens: 200,
            filtered_tokens: 40,
            tokens_saved: 160,
            savings_percent: 80.0,
            timestamp: Utc::now(),
        })
        .await;

    assert_eq!(tracker.total_commands().await, 2);
    assert_eq!(tracker.total_tokens_saved().await, 240);
}

#[tokio::test]
async fn test_format_report() {
    let tracker = RtkTracker::new();

    tracker
        .record_savings(TokenSavings {
            command: "git status".to_string(),
            rewritten_command: "rtk git status".to_string(),
            original_tokens: 100,
            filtered_tokens: 20,
            tokens_saved: 80,
            savings_percent: 80.0,
            timestamp: Utc::now(),
        })
        .await;

    let report = tracker.format_report().await;
    assert!(report.contains("RTK Token Savings Report"));
    assert!(report.contains("Total Commands: 1"));
    assert!(report.contains("git"));
}
