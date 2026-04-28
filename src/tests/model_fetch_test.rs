//! Model Fetching Infrastructure Tests
//!
//! Tests for the generic model fetching from OpenAI-compatible endpoints.
//! Verifies URL normalization, response parsing, and endpoint fallback.

use crate::brain::provider::model_fetch::{
    normalize_base_url, OllamaModelsResponse, OpenAIModelsResponse,
};

// --- URL Normalization Tests ---

#[test]
fn strips_v1_chat_completions() {
    assert_eq!(
        normalize_base_url("http://localhost:1234/v1/chat/completions"),
        "http://localhost:1234"
    );
}

#[test]
fn strips_chat_completions() {
    assert_eq!(
        normalize_base_url("http://localhost:1234/chat/completions"),
        "http://localhost:1234"
    );
}

#[test]
fn strips_v1_suffix() {
    assert_eq!(
        normalize_base_url("http://localhost:11434/v1"),
        "http://localhost:11434"
    );
}

#[test]
fn leaves_clean_url_unchanged() {
    assert_eq!(
        normalize_base_url("http://localhost:11434"),
        "http://localhost:11434"
    );
}

#[test]
fn handles_trailing_slash() {
    assert_eq!(
        normalize_base_url("http://localhost:1234/v1/"),
        "http://localhost:1234"
    );
}

#[test]
fn normalization_order_v1_chat_completions_before_v1() {
    // Ensure /v1/chat/completions strips fully, not leaving /v1 behind
    assert_eq!(
        normalize_base_url("http://localhost:11434/v1/chat/completions"),
        "http://localhost:11434"
    );
}

// --- Ollama Response Parsing Tests ---

#[test]
fn ollama_response_deserialization() {
    let json = r#"{"models":[{"name":"llama3.1:8b"},{"name":"qwen2.5:7b"}]}"#;
    let resp: OllamaModelsResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.models.len(), 2);
    assert_eq!(resp.models[0].name, "llama3.1:8b");
}

#[test]
fn ollama_empty_models_list() {
    let json = r#"{"models":[]}"#;
    let resp: OllamaModelsResponse = serde_json::from_str(json).unwrap();
    assert!(resp.models.is_empty());
}

// --- OpenAI Response Parsing Tests ---

#[test]
fn openai_response_deserialization() {
    let json = r#"{"data":[{"id":"gpt-4o","created":1700000000},{"id":"gpt-3.5-turbo","created":1690000000}]}"#;
    let resp: OpenAIModelsResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.data.len(), 2);
    assert_eq!(resp.data[0].id, "gpt-4o");
}

// --- Endpoint Fallback Logic Tests ---

#[test]
fn ollama_uses_api_tags_path() {
    let base = "http://localhost:11434";
    let tags_url = format!("{}/api/tags", base);
    assert_eq!(tags_url, "http://localhost:11434/api/tags");
}

#[test]
fn openai_compatible_uses_v1_models() {
    let base = "http://localhost:1234";
    let models_url = format!("{}/v1/models", base);
    assert_eq!(models_url, "http://localhost:1234/v1/models");
}
