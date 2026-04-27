//! Generic model fetching from OpenAI-compatible endpoints.
//!
//! Provides a reusable function to fetch model lists from any
//! OpenAI-compatible `/v1/models` endpoint or Ollama `/api/tags`.
//! Used by custom providers, Ollama, onboarding, and the /models dialog.

use reqwest::Client;
use std::time::Duration;

/// Response shape for OpenAI-compatible `/v1/models` endpoint.
#[derive(serde::Deserialize)]
struct OpenAIModelsResponse {
    data: Vec<OpenAIModelEntry>,
}

#[derive(serde::Deserialize)]
struct OpenAIModelEntry {
    id: String,
    #[serde(default)]
    created: i64,
}

/// Response shape for Ollama `/api/tags` endpoint.
#[derive(serde::Deserialize)]
struct OllamaModelsResponse {
    models: Vec<OllamaModelEntry>,
}

#[derive(serde::Deserialize)]
struct OllamaModelEntry {
    name: String,
}

/// Fetch model names from an OpenAI-compatible endpoint.
///
/// Strategy:
/// 1. Try `{base_url}/v1/models` (OpenAI standard)
/// 2. Fall back to `{base_url}/api/tags` (Ollama native)
///
/// Returns sorted model names (newest first by `created` timestamp for OpenAI,
/// alphabetical for Ollama). Returns empty vec on failure.
pub async fn fetch_models_from_endpoint(base_url: &str, api_key: Option<&str>) -> Vec<String> {
    // Normalize base_url: strip trailing slash and any path suffix
    let base = base_url.trim_end_matches('/');
    let base = base
        .strip_suffix("/v1/chat/completions")
        .or_else(|| base.strip_suffix("/chat/completions"))
        .or_else(|| base.strip_suffix("/v1"))
        .unwrap_or(base);

    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .ok();

    let Some(client) = client else {
        tracing::debug!("[model_fetch] failed to build HTTP client");
        return Vec::new();
    };

    // Attempt 1: OpenAI-compatible /v1/models
    let models_url = format!("{}/v1/models", base);
    tracing::debug!("[model_fetch] trying OpenAI-compatible: {}", models_url);

    let mut req = client.get(&models_url);
    if let Some(key) = api_key
        && !key.is_empty()
    {
        req = req.header("Authorization", format!("Bearer {}", key));
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<OpenAIModelsResponse>().await {
            Ok(body) if !body.data.is_empty() => {
                let mut entries = body.data;
                entries.sort_by_key(|e| std::cmp::Reverse(e.created));
                let models: Vec<String> = entries.into_iter().map(|m| m.id).collect();
                tracing::info!(
                    "[model_fetch] fetched {} models from {}",
                    models.len(),
                    models_url
                );
                return models;
            }
            Ok(_) => {
                tracing::debug!("[model_fetch] /v1/models returned empty list");
            }
            Err(e) => {
                tracing::debug!("[model_fetch] /v1/models parse error: {}", e);
            }
        },
        Ok(resp) => {
            tracing::debug!(
                "[model_fetch] /v1/models returned {} — trying Ollama",
                resp.status()
            );
        }
        Err(e) => {
            tracing::debug!(
                "[model_fetch] /v1/models fetch failed: {} — trying Ollama",
                e
            );
        }
    }

    // Attempt 2: Ollama /api/tags
    let ollama_url = format!("{}/api/tags", base);
    tracing::debug!("[model_fetch] trying Ollama: {}", ollama_url);

    let mut req = client.get(&ollama_url);
    if let Some(key) = api_key
        && !key.is_empty()
    {
        req = req.header("Authorization", format!("Bearer {}", key));
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<OllamaModelsResponse>().await {
            Ok(body) if !body.models.is_empty() => {
                let mut models: Vec<String> = body.models.into_iter().map(|m| m.name).collect();
                models.sort();
                tracing::info!(
                    "[model_fetch] fetched {} models from {}",
                    models.len(),
                    ollama_url
                );
                return models;
            }
            Ok(_) => {
                tracing::debug!("[model_fetch] /api/tags returned empty list");
            }
            Err(e) => {
                tracing::debug!("[model_fetch] /api/tags parse error: {}", e);
            }
        },
        Ok(resp) => {
            tracing::debug!("[model_fetch] /api/tags returned {}", resp.status());
        }
        Err(e) => {
            tracing::debug!("[model_fetch] /api/tags fetch failed: {}", e);
        }
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_url_normalization() {
        // These are unit tests for the normalization logic.
        // We can't test actual HTTP calls without a mock server.
        let cases = [
            (
                "http://localhost:11434/v1/chat/completions",
                "http://localhost:11434",
            ),
            ("http://localhost:11434/v1", "http://localhost:11434"),
            ("http://localhost:11434/", "http://localhost:11434"),
            ("http://localhost:11434", "http://localhost:11434"),
            (
                "https://api.openai.com/v1/chat/completions",
                "https://api.openai.com",
            ),
            (
                "https://openrouter.ai/api/v1/chat/completions",
                "https://openrouter.ai/api",
            ),
        ];

        for (input, expected) in cases {
            let base = input.trim_end_matches('/');
            let base = base
                .strip_suffix("/v1/chat/completions")
                .or_else(|| base.strip_suffix("/chat/completions"))
                .or_else(|| base.strip_suffix("/v1"))
                .unwrap_or(base);
            assert_eq!(base, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_ollama_response_deserialization() {
        let json = r#"{"models":[{"name":"llama3.1:8b"},{"name":"qwen2.5:7b"}]}"#;
        let resp: OllamaModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.models.len(), 2);
        assert_eq!(resp.models[0].name, "llama3.1:8b");
    }

    #[test]
    fn test_openai_response_deserialization() {
        let json = r#"{"data":[{"id":"gpt-4o","created":1700000000},{"id":"gpt-3.5-turbo","created":1690000000}]}"#;
        let resp: OpenAIModelsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.data.len(), 2);
        assert_eq!(resp.data[0].id, "gpt-4o");
    }
}
