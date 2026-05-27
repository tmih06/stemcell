//! Tests for `src/rtk/rewrite.rs` — moved out of an inline `#[cfg(test)]
//! mod tests` block so the project keeps every test under `src/tests/`.

use crate::rtk::rewrite::{first_command_token, is_rtk_supported};
use crate::rtk::{is_rtk_available, rewrite_command};

#[test]
fn test_first_command_token_simple() {
    assert_eq!(first_command_token("git status"), "git");
    assert_eq!(first_command_token("cargo build"), "cargo");
    assert_eq!(first_command_token("echo hello"), "echo");
}

#[test]
fn test_first_command_token_with_env() {
    assert_eq!(first_command_token("FOO=bar git status"), "git");
    assert_eq!(first_command_token("PATH=/usr/bin cargo test"), "cargo");
}

#[test]
fn test_rtk_supported() {
    assert!(is_rtk_supported("git"));
    assert!(is_rtk_supported("cargo"));
    assert!(is_rtk_supported("npm"));
    assert!(is_rtk_supported("docker"));
    assert!(!is_rtk_supported("echo"));
    assert!(!is_rtk_supported("cat"));
    assert!(!is_rtk_supported("rm"));
}

#[test]
fn test_rtk_blocklist() {
    assert!(!is_rtk_supported("sudo"));
    assert!(!is_rtk_supported("ssh"));
    assert!(!is_rtk_supported("vim"));
    assert!(!is_rtk_supported("rtk"));
}

#[test]
fn test_rtk_supported_with_path() {
    assert!(is_rtk_supported("/usr/bin/git"));
    assert!(is_rtk_supported("/usr/local/bin/cargo"));
}

#[tokio::test]
async fn test_rewrite_git_status() {
    if !is_rtk_available().await {
        return;
    }
    let result = rewrite_command("git status").await;
    assert!(result.is_some());
    let r = result.unwrap();
    assert!(r.was_rewritten);
    assert_eq!(r.rewritten_command, "rtk git status");
}

#[tokio::test]
async fn test_rewrite_echo_not_supported() {
    let result = rewrite_command("echo hello").await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_rewrite_already_rtk() {
    let result = rewrite_command("rtk git status").await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_rewrite_sudo_blocked() {
    let result = rewrite_command("sudo git pull").await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_rewrite_chained_command() {
    if !is_rtk_available().await {
        return;
    }
    let result = rewrite_command("git status && echo done").await;
    assert!(result.is_some());
    let r = result.unwrap();
    assert_eq!(r.rewritten_command, "rtk git status && echo done");
}

#[tokio::test]
async fn test_rewrite_empty_command() {
    let result = rewrite_command("").await;
    assert!(result.is_none());
}

#[tokio::test]
async fn test_rewrite_cargo_test() {
    if !is_rtk_available().await {
        return;
    }
    let result = rewrite_command("cargo test --release").await;
    assert!(result.is_some());
    let r = result.unwrap();
    assert_eq!(r.rewritten_command, "rtk cargo test --release");
}

#[tokio::test]
async fn test_rewrite_npm_install() {
    if !is_rtk_available().await {
        return;
    }
    let result = rewrite_command("npm install express").await;
    assert!(result.is_some());
    let r = result.unwrap();
    assert_eq!(r.rewritten_command, "rtk npm install express");
}

#[tokio::test]
async fn test_rewrite_env_prefix() {
    if !is_rtk_available().await {
        return;
    }
    let result = rewrite_command("RUST_LOG=debug cargo build").await;
    assert!(result.is_some());
    let r = result.unwrap();
    assert_eq!(r.rewritten_command, "rtk RUST_LOG=debug cargo build");
}

// ─── No-block regression test (issue #125) ──────────────────────────
//
// Pin the async contract: `is_rtk_available` and `rewrite_command` must
// not block any tokio worker. The original sync `std::process::Command`
// implementation deadlocked the runtime when called from a single-worker
// scheduler under concurrent load — that was the Telegram-daemon-hang
// pattern in issue #125. This test runs both call paths concurrently on
// a `current_thread` flavour runtime; a regression that re-introduces a
// sync `which`/`std::process::Command` call would hang the test instead
// of returning, and the configured 5-second timeout would fail it.

#[test]
fn concurrent_rtk_calls_never_block_single_worker_runtime() {
    use std::time::Duration;
    use tokio::time::timeout;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current_thread runtime");

    rt.block_on(async {
        let mut handles = Vec::new();
        for _ in 0..32 {
            handles.push(tokio::spawn(async {
                let _ = is_rtk_available().await;
                let _ = rewrite_command("git status").await;
                let _ = rewrite_command("echo hello").await;
                let _ = rewrite_command("RUST_LOG=debug cargo build").await;
            }));
        }

        let joined = futures::future::join_all(handles);
        timeout(Duration::from_secs(5), joined).await.expect(
            "RTK calls hung a single-worker runtime — sync std::process::Command \
             was likely reintroduced (issue #125 regression)",
        );
    });
}
