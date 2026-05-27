//! End-to-end test for the auto-cache wire-up in custom-provider factories.
//!
//! Exercises `auto_qwen_cache_transform` + `chain_body_transforms` against
//! synthetic request bodies that mirror what `OpenAIProvider::encode_body`
//! produces. Verifies the transform engages for Qwen-shaped targets and
//! stays a no-op everywhere else — the wire bytes guarantee we promise to
//! non-Qwen backends.
//!
//! Tests the body-transform layer directly; the factory wiring that
//! installs it on `OpenAIProvider` is covered by the existing custom-provider
//! integration tests.

use crate::brain::provider::factory::{auto_qwen_cache_transform, chain_body_transforms};
use serde_json::{Value, json};

/// Minimal OpenAI chat-completions body shape with one system, one user,
/// and one tool — exactly the surface `qwen_body_transform` decorates.
fn sample_body(model: &str) -> Value {
    json!({
        "model": model,
        "stream": true,
        "messages": [
            { "role": "system", "content": "You are helpful." },
            { "role": "user", "content": "Say hi." }
        ],
        "tools": [
            { "type": "function", "function": { "name": "bash", "parameters": {} } }
        ],
    })
}

fn first_msg_cache(body: &Value, idx: usize) -> Option<&str> {
    body.get("messages")?
        .get(idx)?
        .get("content")?
        .get(0)?
        .get("cache_control")?
        .get("type")?
        .as_str()
}

fn last_tool_cache(body: &Value) -> Option<&str> {
    let tools = body.get("tools")?.as_array()?;
    tools.last()?.get("cache_control")?.get("type")?.as_str()
}

// ─── Auto-enable: URL signal ──────────────────────────────────────────

#[test]
fn dashscope_url_triggers_cache_markers_on_any_model() {
    let transform =
        auto_qwen_cache_transform("https://dashscope.aliyuncs.com/v1/chat/completions".to_string());
    let out = transform(sample_body("some-non-qwen-model"));
    assert_eq!(
        first_msg_cache(&out, 0),
        Some("ephemeral"),
        "system message must carry ephemeral cache marker"
    );
    assert_eq!(
        last_tool_cache(&out),
        Some("ephemeral"),
        "last tool must carry ephemeral cache marker (streaming)"
    );
}

#[test]
fn dialagram_url_user_repro_2026_05_27() {
    // Verbatim reproduction of the user's setup from the v0.3.29 audit.
    let transform =
        auto_qwen_cache_transform("https://dialagram.me/router/v1/chat/completions".to_string());
    let out = transform(sample_body("qwen-3.7-max-thinking"));
    assert_eq!(first_msg_cache(&out, 0), Some("ephemeral"));
    assert_eq!(last_tool_cache(&out), Some("ephemeral"));
}

// ─── Auto-enable: model signal (even with non-qwen URL) ───────────────

#[test]
fn qwen_model_on_arbitrary_url_triggers_cache_markers() {
    // A custom provider could in principle proxy Qwen through any host.
    // If the model name says qwen, we mark.
    let transform = auto_qwen_cache_transform(
        "https://my-custom-proxy.example.com/v1/chat/completions".to_string(),
    );
    let out = transform(sample_body("qwen-3.7-max-thinking"));
    assert_eq!(first_msg_cache(&out, 0), Some("ephemeral"));
    assert_eq!(last_tool_cache(&out), Some("ephemeral"));
}

// ─── No-op: neither signal fires ──────────────────────────────────────

#[test]
fn openai_url_with_gpt_model_stays_unmodified() {
    let transform =
        auto_qwen_cache_transform("https://api.openai.com/v1/chat/completions".to_string());
    let input = sample_body("gpt-4o");
    let out = transform(input.clone());

    // Body must be byte-for-byte identical when neither signal matches —
    // this is the contract we make to non-Qwen backends.
    assert_eq!(
        out, input,
        "non-qwen request body must pass through unchanged"
    );
}

#[test]
fn deepseek_url_with_deepseek_model_stays_unmodified() {
    let transform =
        auto_qwen_cache_transform("https://api.deepseek.com/v1/chat/completions".to_string());
    let input = sample_body("deepseek-v3.2");
    let out = transform(input.clone());
    assert_eq!(out, input);
}

#[test]
fn anthropic_url_with_claude_model_stays_unmodified() {
    let transform = auto_qwen_cache_transform("https://api.anthropic.com/v1/messages".to_string());
    let input = sample_body("claude-sonnet-4-6");
    let out = transform(input.clone());
    assert_eq!(out, input);
}

// ─── Tongyi / q3 alias policy (strict-prefix non-match) ───────────────

#[test]
fn tongyi_alias_on_neutral_url_does_not_trigger() {
    let transform =
        auto_qwen_cache_transform("https://example.com/v1/chat/completions".to_string());
    let input = sample_body("tongyi-qianwen-max");
    let out = transform(input.clone());
    assert_eq!(
        out, input,
        "tongyi alias must NOT auto-enable (strict prefix policy)"
    );
}

// ─── Composition with local-thinking transform ────────────────────────

#[test]
fn chain_with_local_thinking_applies_both_on_qwen_target() {
    let local_thinking = std::sync::Arc::new(|mut body: Value| {
        if let Some(obj) = body.as_object_mut() {
            obj.insert(
                "chat_template_kwargs".to_string(),
                json!({ "enable_thinking": true }),
            );
        }
        body
    });
    let cache = auto_qwen_cache_transform("https://dashscope.aliyuncs.com/v1".to_string());
    let chained = chain_body_transforms(local_thinking, cache);

    let out = chained(sample_body("qwen-3.7-max-thinking"));
    assert_eq!(
        out.get("chat_template_kwargs")
            .and_then(|v| v.get("enable_thinking")),
        Some(&Value::Bool(true)),
        "local_thinking_body_transform must apply"
    );
    assert_eq!(
        first_msg_cache(&out, 0),
        Some("ephemeral"),
        "cache transform must also apply"
    );
}

#[test]
fn chain_with_local_thinking_only_thinking_applies_on_non_qwen() {
    let local_thinking = std::sync::Arc::new(|mut body: Value| {
        if let Some(obj) = body.as_object_mut() {
            obj.insert(
                "chat_template_kwargs".to_string(),
                json!({ "enable_thinking": false }),
            );
        }
        body
    });
    let cache = auto_qwen_cache_transform("http://localhost:8080/v1".to_string());
    let chained = chain_body_transforms(local_thinking, cache);

    let out = chained(sample_body("llama-3.1-70b"));
    assert_eq!(
        out.get("chat_template_kwargs")
            .and_then(|v| v.get("enable_thinking")),
        Some(&Value::Bool(false))
    );
    assert!(
        first_msg_cache(&out, 0).is_none(),
        "cache transform must NOT touch non-qwen body"
    );
}

// ─── Dedup logging: same (url, model) only logs once ──────────────────
//
// We don't intercept the log line (no tracing-subscriber wiring in the
// test harness) — instead we verify the transform stays functionally
// correct across repeat calls. The dedup is a soft observability detail;
// the wire behavior is what users feel.

#[test]
fn repeat_calls_with_same_model_keep_applying_markers() {
    let transform =
        auto_qwen_cache_transform("https://dialagram.me/router/v1/chat/completions".to_string());
    for _ in 0..5 {
        let out = transform(sample_body("qwen-3.7-max-thinking"));
        assert_eq!(first_msg_cache(&out, 0), Some("ephemeral"));
        assert_eq!(last_tool_cache(&out), Some("ephemeral"));
    }
}
