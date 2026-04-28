//! Generic model fetching from OpenAI-compatible endpoints.
//!
//! Provides a reusable function to fetch model lists from any
//! OpenAI-compatible `/v1/models` endpoint or Ollama `/api/tags`.
//! Used by custom providers, Ollama, onboarding, and the /models dialog.

use reqwest::Client;
use std::time::Duration;

/// Response shape for OpenAI-compatible `/v1/models` endpoint.
#[derive(serde::Deserialize)]
pub(crate) struct OpenAIModelsResponse {
    pub data: Vec<OpenAIModelEntry>,
}

#[derive(serde::Deserialize)]
pub(crate) struct OpenAIModelEntry {
    pub id: String,
    #[serde(default)]
    pub created: i64,
}

/// Response shape for Ollama `/api/tags` endpoint.
#[derive(serde::Deserialize)]
pub(crate) struct OllamaModelsResponse {
    pub models: Vec<OllamaModelEntry>,
}

#[derive(serde::Deserialize)]
pub(crate) struct OllamaModelEntry {
    pub name: String,
}

/// Normalize a base URL by stripping trailing slashes and common API path suffixes.
pub(crate) fn normalize_base_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    base.strip_suffix("/v1/chat/completions")
        .or_else(|| base.strip_suffix("/chat/completions"))
        .or_else(|| base.strip_suffix("/v1"))
        .unwrap_or(base)
        .to_string()
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
    let base = normalize_base_url(base_url);

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
