//! Tests for `looks_like_qwen_target` — the heuristic that decides whether
//! a custom provider gets `cache_control: {type: "ephemeral"}` markers
//! auto-applied to its outgoing requests.
//!
//! Detection trigger is OR-shaped: base_url match OR model match wins.
//! Match is strict (lowercase prefix on model, substring on URL) so that
//! adding markers to a non-matching backend stays at zero rate.

use crate::brain::provider::qwen::looks_like_qwen_target;

// ─── URL matches (any model) ──────────────────────────────────────────

#[test]
fn url_match_dashscope_aliyuncs() {
    assert!(looks_like_qwen_target(
        "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions",
        "gpt-4-via-shim",
    ));
}

#[test]
fn url_match_dashscope_intl() {
    assert!(looks_like_qwen_target(
        "https://dashscope-intl.aliyuncs.com/compatible-mode/v1/chat/completions",
        "anything",
    ));
}

#[test]
fn url_match_aliyun_root() {
    assert!(looks_like_qwen_target(
        "https://api.aliyun.com/v1/chat/completions",
        "anything",
    ));
}

#[test]
fn url_match_dialagram() {
    // The user's actual custom provider as of 2026-05-27.
    assert!(looks_like_qwen_target(
        "https://dialagram.me/router/v1/chat/completions",
        "anything",
    ));
}

#[test]
fn url_match_is_case_insensitive() {
    assert!(looks_like_qwen_target(
        "HTTPS://DASHSCOPE.ALIYUNCS.COM/v1/chat/completions",
        "anything",
    ));
}

// ─── Model matches (any URL) ──────────────────────────────────────────

#[test]
fn model_match_qwen_3_7_max_thinking() {
    assert!(looks_like_qwen_target(
        "https://example.com/v1/chat/completions",
        "qwen-3.7-max-thinking",
    ));
}

#[test]
fn model_match_qwen3_max() {
    assert!(looks_like_qwen_target(
        "https://example.com/v1/chat/completions",
        "qwen3-max",
    ));
}

#[test]
fn model_match_qwen_vl_plus() {
    assert!(looks_like_qwen_target(
        "https://example.com/v1/chat/completions",
        "qwen-vl-plus",
    ));
}

#[test]
fn model_match_is_case_insensitive() {
    assert!(looks_like_qwen_target(
        "https://example.com/v1/chat/completions",
        "Qwen-3.7-Max",
    ));
}

// ─── Negative cases — must NOT auto-enable ────────────────────────────

#[test]
fn does_not_match_openai_gpt() {
    assert!(!looks_like_qwen_target(
        "https://api.openai.com/v1/chat/completions",
        "gpt-4o",
    ));
}

#[test]
fn does_not_match_anthropic_claude() {
    assert!(!looks_like_qwen_target(
        "https://api.anthropic.com/v1/messages",
        "claude-sonnet-4-6",
    ));
}

#[test]
fn does_not_match_gemini() {
    assert!(!looks_like_qwen_target(
        "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
        "gemini-2.5-flash",
    ));
}

#[test]
fn does_not_match_deepseek() {
    assert!(!looks_like_qwen_target(
        "https://api.deepseek.com/v1/chat/completions",
        "deepseek-v3.2",
    ));
}

#[test]
fn does_not_match_kimi() {
    assert!(!looks_like_qwen_target(
        "https://api.moonshot.cn/v1/chat/completions",
        "kimi-k2-thinking",
    ));
}

// Strict-prefix policy: `tongyi-*` and `q3-*` aliases do NOT match. Users
// running those through a custom provider can rename the model entry.
#[test]
fn strict_prefix_tongyi_alias_does_not_match() {
    assert!(!looks_like_qwen_target(
        "https://example.com/v1/chat/completions",
        "tongyi-qianwen-max",
    ));
}

#[test]
fn strict_prefix_q3_alias_does_not_match() {
    assert!(!looks_like_qwen_target(
        "https://example.com/v1/chat/completions",
        "q3-coder-32b",
    ));
}

// Substring `qwen` mid-string does NOT match (must be prefix).
#[test]
fn mid_string_qwen_does_not_match() {
    assert!(!looks_like_qwen_target(
        "https://example.com/v1/chat/completions",
        "my-qwen-finetune",
    ));
}

// Empty string sanity.
#[test]
fn empty_inputs_do_not_match() {
    assert!(!looks_like_qwen_target("", ""));
}
