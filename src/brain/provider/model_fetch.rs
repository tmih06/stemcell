//! Generic model fetching from OpenAI-compatible endpoints.
//!
//! Provides a reusable function to fetch model lists from any
//! OpenAI-compatible `/v1/models` endpoint or Ollama `/api/tags`.
//! Used by custom providers, Ollama, onboarding, and the /models dialog.

use reqwest::Client;
use std::time::Duration;

/// How long a cached model list is considered fresh before a re-fetch is
/// attempted. Model rosters change on the order of weeks, so an hour keeps
/// the list current without hammering provider APIs on every `/models` call.
pub const DEFAULT_MODEL_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// Fetch a model list through the shared disk-backed cache
/// ([`crate::startup::model_cache`], also warmed by the startup `fetch-models`
/// job). `key` identifies the provider (e.g. its name or base URL). The flow:
/// 1. Fresh cache hit (within `ttl`) → return it, no network.
/// 2. Otherwise run `fetch` (a live API call). On success, persist + return.
/// 3. On fetch failure, fall back to the stale cache entry if one exists.
///
/// This is what lets every provider track new model releases automatically
/// while staying fast and resilient to transient network failures.
pub async fn cached_or_fetch<F, Fut>(key: &str, ttl: Duration, fetch: F) -> Vec<String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Vec<String>>,
{
    use crate::startup::model_cache;

    let (cached, fresh) = model_cache::warm_start(key, ttl.as_secs());
    if fresh && let Some(models) = cached {
        tracing::debug!("[model_fetch] cache hit for '{}'", key);
        return models;
    }

    let fetched = fetch().await;
    if !fetched.is_empty() {
        model_cache::store(key, fetched.clone());
        return fetched;
    }

    // Fetch failed/empty — serve stale cache rather than nothing.
    // `warm_start` only returns `Some` for a non-empty entry.
    if cached.is_some() {
        tracing::debug!("[model_fetch] serving stale cache for '{}'", key);
    }
    cached.unwrap_or_default()
}

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

/// Natural version sort: extracts numeric segments and compares numerically.
/// So "3.7" > "3.6" > "3.5", "32B" > "14B" > "8B", "max" > "plus" by position.
/// Returns newest-first ordering (descending).
fn version_sort_key(name: &str) -> Vec<(isize, String)> {
    let lower = name.to_lowercase();
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut in_number = false;

    for ch in lower.chars() {
        if ch.is_ascii_digit() {
            if !in_number && !current.is_empty() {
                segments.push((-1, std::mem::take(&mut current)));
            }
            in_number = true;
            current.push(ch);
        } else {
            if in_number && !current.is_empty() {
                let n: isize = current.parse().unwrap_or(0);
                segments.push((n, String::new()));
                current.clear();
            }
            in_number = false;
            current.push(ch);
        }
    }
    if !current.is_empty() {
        if in_number {
            let n: isize = current.parse().unwrap_or(0);
            segments.push((n, String::new()));
        } else {
            segments.push((-1, current));
        }
    }

    // Negate numeric segments for descending order (newest first)
    segments
        .into_iter()
        .map(|(n, s)| if n >= 0 { (-n, s) } else { (n, s) })
        .collect()
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

    let Some(client) = http_client() else {
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
                // Use created timestamps if meaningful (not all zero/identical), else version sort
                let timestamps_useful = entries.iter().any(|e| e.created > 0)
                    && entries.windows(2).any(|w| w[0].created != w[1].created);
                if timestamps_useful {
                    entries.sort_by_key(|e| std::cmp::Reverse(e.created));
                    let models: Vec<String> = entries.into_iter().map(|m| m.id).collect();
                    return models;
                }
                let mut models: Vec<String> = entries.into_iter().map(|m| m.id).collect();
                models.sort_by_key(|a| version_sort_key(a));
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
                models.sort_by_key(|a| version_sort_key(a));
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

fn http_client() -> Option<Client> {
    Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .ok()
}

/// Response shape for Anthropic's `/v1/models`. Unlike OpenAI, the release
/// timestamp is `created_at` (an RFC3339 string), not a numeric `created`.
#[derive(serde::Deserialize)]
struct AnthropicModelsResponse {
    data: Vec<AnthropicModelEntry>,
}

#[derive(serde::Deserialize)]
struct AnthropicModelEntry {
    id: String,
    #[serde(default)]
    created_at: String,
}

/// Fetch Claude model ids live from Anthropic's `/v1/models`. Anthropic
/// authenticates with `x-api-key` (or a `Bearer` + `anthropic-beta` header
/// for OAuth tokens) and requires the `anthropic-version` header — it is
/// not a plain OpenAI-compatible endpoint, so it needs its own fetcher.
/// Returns newest-first; empty on failure.
pub async fn fetch_anthropic_models(api_key: Option<&str>) -> Vec<String> {
    let Some(client) = http_client() else {
        return Vec::new();
    };
    let mut req = client
        .get("https://api.anthropic.com/v1/models?limit=100")
        .header("anthropic-version", "2023-06-01");
    if let Some(key) = api_key
        && !key.is_empty()
    {
        if key.starts_with("sk-ant-oat") {
            req = req
                .header("Authorization", format!("Bearer {}", key))
                .header("anthropic-beta", "oauth-2025-04-20");
        } else {
            req = req.header("x-api-key", key);
        }
    }
    match req.send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<AnthropicModelsResponse>().await
        {
            Ok(body) if !body.data.is_empty() => {
                let mut entries = body.data;
                // Anthropic returns newest-first already, but sort defensively
                // on `created_at` (an RFC3339 string that orders chronologically
                // as text) so ordering does not depend on API response order.
                entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                entries.into_iter().map(|m| m.id).collect()
            }
            _ => Vec::new(),
        },
        Ok(resp) => {
            tracing::debug!(
                "[model_fetch] anthropic /v1/models returned {}",
                resp.status()
            );
            Vec::new()
        }
        Err(e) => {
            tracing::debug!("[model_fetch] anthropic fetch failed: {}", e);
            Vec::new()
        }
    }
}

#[derive(serde::Deserialize)]
struct GeminiModelEntry {
    name: String,
    #[serde(default, rename = "supportedGenerationMethods")]
    supported_generation_methods: Vec<String>,
}

#[derive(serde::Deserialize)]
struct GeminiModelsResponse {
    models: Vec<GeminiModelEntry>,
}

/// Fetch Gemini model ids live from the generativelanguage API. Uses the
/// `x-goog-api-key` header and a Gemini-specific response shape
/// (`models[].name = "models/gemini-..."`), filtering to models that
/// support `generateContent`. Returns newest-first; empty on failure.
pub async fn fetch_gemini_models(api_key: Option<&str>) -> Vec<String> {
    let Some(key) = api_key.filter(|k| !k.is_empty()) else {
        return Vec::new();
    };
    let Some(client) = http_client() else {
        return Vec::new();
    };
    let url = "https://generativelanguage.googleapis.com/v1beta/models?pageSize=200";
    match client.get(url).header("x-goog-api-key", key).send().await {
        Ok(resp) if resp.status().is_success() => match resp.json::<GeminiModelsResponse>().await {
            Ok(body) => {
                let mut models: Vec<String> = body
                    .models
                    .into_iter()
                    .filter(|m| {
                        m.supported_generation_methods.is_empty()
                            || m.supported_generation_methods
                                .iter()
                                .any(|g| g == "generateContent")
                    })
                    .map(|m| {
                        m.name
                            .strip_prefix("models/")
                            .unwrap_or(&m.name)
                            .to_string()
                    })
                    .collect();
                models.sort_by_key(|a| version_sort_key(a));
                models
            }
            Err(e) => {
                tracing::debug!("[model_fetch] gemini parse error: {}", e);
                Vec::new()
            }
        },
        Ok(resp) => {
            tracing::debug!("[model_fetch] gemini models returned {}", resp.status());
            Vec::new()
        }
        Err(e) => {
            tracing::debug!("[model_fetch] gemini fetch failed: {}", e);
            Vec::new()
        }
    }
}
