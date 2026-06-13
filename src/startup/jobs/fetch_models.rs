//! Startup job: fetch every known provider's model list and warm the on-disk
//! cache so the `/models` dialog opens instantly.
//!
//! Strategy (two passes, both respect 24h cache freshness):
//!
//! 1. Fetch **models.dev** (models.dev/api.json) — a free, no-auth, community
//!    catalog with curated per-model capability metadata (display name,
//!    `tool_call`, `reasoning`, modalities, cost, context limits).  Model IDs
//!    are already in their native form, so no prefix-stripping is needed.
//!
//! 2. For each credentialled provider, fetch from its **own** live API so
//!    authoritative results overwrite the models.dev-derived data.  Live
//!    fetches know only model ids; [`model_cache::store`] preserves any
//!    models.dev metadata already cached for those ids.
//!
//! Both passes skip providers whose cache entries are fresh (within 24h).
//! Network-bound — exactly the kind of slow work that belongs in a background
//! startup job. On failure it logs and leaves any prior cache intact.

use std::collections::{HashMap, HashSet};

use crate::startup::job::{StartupContext, StartupJob};
use crate::startup::model_cache::{self, CachedModel, ModelCost, ModelLimit};
use async_trait::async_trait;

pub struct FetchModelsJob;

#[async_trait]
impl StartupJob for FetchModelsJob {
    fn name(&self) -> &'static str {
        "fetch-models"
    }

    async fn run(&self, ctx: &StartupContext) -> anyhow::Result<Option<String>> {
        let config = &ctx.config;

        // Merge API keys from keys.toml so that API-key providers the user
        // has credentials for are discovered (not just always-enabled CLI
        // providers like codex / opencode-cli).  The context config may not
        // have keys merged yet (e.g. when loaded via --config), but the
        // startup warm-up should still cover all credentialed providers.
        let creds = {
            let mut p = config.providers.clone();
            if let Ok(keys) = crate::config::load_keys_from_file() {
                p = crate::config::merge_provider_keys(p, keys.providers);
            }
            p
        };

        // Pass 1: seed the cache from models.dev (models.dev/api.json) — a
        // free, no-auth catalog with curated per-model capability metadata.
        // Only fetches when at least one known provider is stale.
        let ttl = model_cache::FRESH_TTL_SECS;
        let credentials = crate::utils::providers::configured_providers(&creds);

        // Snapshot which credentialed providers were already fresh BEFORE this
        // run seeds anything.  Pass 2 below uses this — not the live cache — to
        // decide what to skip, so a models.dev entry Pass 1 just wrote this run
        // is not mistaken for a cache that's still fresh from a previous
        // startup.  Without this, Pass 1 makes every models.dev-covered
        // provider look fresh and Pass 2's authoritative live fetch never runs.
        let pre_run_fresh: HashSet<String> = credentials
            .iter()
            .map(|(provider, _)| provider.clone())
            .filter(|provider| model_cache::is_fresh(provider, ttl))
            .collect();

        let mut warmed: HashSet<String> = HashSet::new();
        if needs_catalog_refresh() {
            for (pid, models) in fetch_modelsdev_catalog().await {
                model_cache::store_rich(&pid, models);
                warmed.insert(pid);
            }
        } else {
            // Cache is still fresh — note what we would have warmed so the
            // report line is still accurate.
            let cache = model_cache::load();
            for our_id in catalog_known_ids() {
                if cache.contains_key(our_id) {
                    warmed.insert(our_id.to_string());
                }
            }
        }

        // Pass 2: for each credentialled provider, fetch from its own live
        // API.  Authoritative results overwrite the ModelDB-derived data.
        // Skip only providers whose cache was already fresh before this run
        // (within 24 hours) — see `pre_run_fresh` above.
        for (provider, _display) in &credentials {
            if provider.starts_with("custom:") {
                continue;
            }

            // Skip if the cache was already fresh at the start of this run.
            if pre_run_fresh.contains(provider) {
                warmed.insert(provider.clone());
                continue;
            }

            let Some(provider_index) = crate::utils::providers::tui_index_for_id(provider) else {
                tracing::warn!("[startup] fetch-models: no TUI index for '{provider}', skipping");
                continue;
            };
            let api_key = crate::utils::providers::config_for(&creds, provider)
                .and_then(|p| p.api_key.clone());

            let models = crate::tui::onboarding::fetch_provider_models(
                provider_index,
                api_key.as_deref(),
                None,
                None,
            )
            .await;

            if models.is_empty() {
                continue;
            }

            model_cache::store(provider, models);
            warmed.insert(provider.clone());
        }

        if warmed.is_empty() {
            return Ok(Some("no model lists warmed".to_string()));
        }

        // Collect stats from the cache for the report line.
        let cache = model_cache::load();
        let total_models: usize = cache
            .iter()
            .filter(|(pid, _)| warmed.contains(pid.as_str()))
            .map(|(_, e)| e.models.len())
            .sum();
        let mut sorted: Vec<String> = warmed.into_iter().collect();
        sorted.sort();

        Ok(Some(format!(
            "cached {total_models} models for {} provider(s): {}",
            sorted.len(),
            sorted.join(", ")
        )))
    }
}

// ── models.dev catalog ───────────────────────────────────────────────────────

/// The chat TUI is the main interface, so the model picker should only list
/// models capable of conversation + tool use.  models.dev curates a
/// `tool_call` boolean and `modalities.input` array per model, so we keep
/// models that (a) advertise tool calling and (b) accept text input, and drop
/// everything else (image, embedding, audio, rerank, video, …).
///
/// `tool_call` absent is treated as non-tool-capable: models.dev populates it
/// for every conversational model, so a missing flag reliably marks a
/// non-chat surface (embeddings, image generation) rather than an omission.
pub(crate) fn is_chat_capable(tool_call: Option<bool>, input_modalities: &[String]) -> bool {
    let takes_text = input_modalities.is_empty()
        || input_modalities
            .iter()
            .any(|m| m.eq_ignore_ascii_case("text"));
    tool_call == Some(true) && takes_text
}

/// models.dev provider key → our internal provider ID.
const CATALOG_PROVIDER_MAP: &[(&str, &str)] = &[
    ("anthropic", "anthropic"),
    ("openai", "openai"),
    ("google", "gemini"),
    ("minimax", "minimax"),
    ("alibaba", "qwen"),
    ("zai", "zhipu"),
    ("openrouter", "openrouter"),
    ("github-copilot", "github"),
    ("ollama-cloud", "ollama"),
    ("amazon-bedrock", "bedrock"),
    ("google-vertex", "vertex"),
    // OpenCode Zen (/zen/v1) and Go (/zen/go/v1) — listed from models.dev
    // without credentials so the picker shows them even when no key is set.
    // Go in particular is paid-only, so without this seed its cache stays
    // empty and the provider vanishes from the picker entirely.
    ("opencode", "opencode"),
    ("opencode-go", "opencode_go"),
];

/// The internal provider IDs we expect models.dev to cover, derived from the
/// distinct values of [`CATALOG_PROVIDER_MAP`] so the two never drift.
fn catalog_known_ids() -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = Vec::new();
    for (_, our_id) in CATALOG_PROVIDER_MAP {
        if !ids.contains(our_id) {
            ids.push(our_id);
        }
    }
    ids
}

/// True when at least one known provider has a stale or missing cache entry,
/// meaning we should re-fetch from models.dev.
fn needs_catalog_refresh() -> bool {
    let ttl = model_cache::FRESH_TTL_SECS;
    !catalog_known_ids()
        .into_iter()
        .all(|id| model_cache::is_fresh(id, ttl))
}

/// Fetch model metadata from models.dev (https://models.dev/api.json) — a
/// free, no-auth catalog of curated per-model capability data.
///
/// Only returns entries for providers in [`CATALOG_PROVIDER_MAP`].  Unknown
/// providers are silently dropped so the on-disk cache stays lean.  Non-chat
/// models (no tool calling / non-text input) are filtered out per
/// [`is_chat_capable`].
async fn fetch_modelsdev_catalog() -> HashMap<String, Vec<CachedModel>> {
    let url = "https://models.dev/api.json";
    // models.dev sits behind Cloudflare and gzip-compresses this response from
    // ~2.26 MB down to ~190 KB.  Opt in explicitly: reqwest only negotiates
    // gzip when the `gzip` feature is enabled *and* requested here, and it
    // transparently decompresses the body before we parse it.
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .gzip(true)
        .build()
    {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };

    let resp = match client.get(url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return HashMap::new(),
    };

    // Top level is { provider_key: { models: { model_id: {..} } } }.
    let root: HashMap<String, serde_json::Value> = match resp.json().await {
        Ok(v) => v,
        Err(_) => return HashMap::new(),
    };

    let overrides: HashMap<&str, &str> = CATALOG_PROVIDER_MAP.iter().copied().collect();
    let mut catalog: HashMap<String, Vec<CachedModel>> = HashMap::new();

    for (provider_key, provider_val) in &root {
        let our_id = match overrides.get(provider_key.as_str()).copied() {
            Some(id) => id,
            None => continue, // unknown provider, skip to keep cache lean
        };

        let models = match provider_val.get("models").and_then(|m| m.as_object()) {
            Some(m) => m,
            None => continue,
        };

        for (model_id, model_val) in models {
            if model_id.is_empty() {
                continue;
            }

            // Retired models 401 on completion regardless of key — never cache
            // them, or the picker hands the user dead options. Mirrors the live
            // fetch path in `fetch_models_dev_opencode`.
            if model_val.get("status").and_then(|v| v.as_str()) == Some("deprecated") {
                continue;
            }

            let tool_call = model_val.get("tool_call").and_then(|v| v.as_bool());
            let reasoning = model_val.get("reasoning").and_then(|v| v.as_bool());
            let input_modalities: Vec<String> = model_val
                .get("modalities")
                .and_then(|m| m.get("input"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            // The chat TUI is the main interface, so only keep models capable
            // of conversation + tool use.  This authoritative capability check
            // replaces the old id-substring heuristic for catalog-sourced data.
            if !is_chat_capable(tool_call, &input_modalities) {
                continue;
            }

            let name = model_val
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::to_string);

            let cost_obj = model_val.get("cost");
            let cost = ModelCost {
                input: cost_obj
                    .and_then(|c| c.get("input"))
                    .and_then(|v| v.as_f64()),
                output: cost_obj
                    .and_then(|c| c.get("output"))
                    .and_then(|v| v.as_f64()),
                cache_read: cost_obj
                    .and_then(|c| c.get("cache_read"))
                    .and_then(|v| v.as_f64()),
                cache_write: cost_obj
                    .and_then(|c| c.get("cache_write"))
                    .and_then(|v| v.as_f64()),
            };

            let limit_obj = model_val.get("limit");
            let limit = ModelLimit {
                context: limit_obj
                    .and_then(|l| l.get("context"))
                    .and_then(|v| v.as_u64()),
                output: limit_obj
                    .and_then(|l| l.get("output"))
                    .and_then(|v| v.as_u64()),
            };

            catalog
                .entry(our_id.to_string())
                .or_default()
                .push(CachedModel {
                    id: model_id.clone(),
                    name,
                    tool_call,
                    reasoning,
                    input_modalities,
                    cost,
                    limit,
                });
        }
    }

    // Deduplicate by id and sort newest-first within each bucket.
    for models in catalog.values_mut() {
        models.sort_by(|a, b| a.id.cmp(&b.id));
        models.dedup_by(|a, b| a.id == b.id);
        models.reverse();
    }

    catalog
}
