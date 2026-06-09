//! Startup job: fetch every known provider's model list and warm the on-disk
//! cache so the `/models` dialog opens instantly.
//!
//! Strategy (two passes, both respect 24h cache freshness):
//!
//! 1. Fetch **ModelDB** (modeldb.axiom.co) — a free, no-auth, hourly-synced
//!    LiteLLM catalog with 2700+ models from 90 providers.  Model IDs are
//!    already in their native form, so no prefix-stripping is needed.
//!
//! 2. For each credentialled provider, fetch from its **own** live API so
//!    authoritative results overwrite the ModelDB-derived data.
//!
//! Both passes skip providers whose cache entries are fresh (within 24h).
//! Network-bound — exactly the kind of slow work that belongs in a background
//! startup job. On failure it logs and leaves any prior cache intact.

use std::collections::{HashMap, HashSet};

use crate::startup::job::{StartupContext, StartupJob};
use crate::startup::model_cache;
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

        // Pass 1: seed the cache from ModelDB (modeldb.axiom.co) — a free,
        // no-auth, hourly-synced LiteLLM catalog with 2700+ models from 90
        // providers.  Only fetches when at least one known provider is stale.
        let mut warmed: HashSet<String> = HashSet::new();
        if needs_modeldb_refresh() {
            for (pid, models) in fetch_modeldb_catalog().await {
                model_cache::store(&pid, models);
                warmed.insert(pid);
            }
        } else {
            // Cache is still fresh — note what we would have warmed so the
            // report line is still accurate.
            let cache = model_cache::load();
            for our_id in modeldb_known_ids() {
                if cache.contains_key(our_id) {
                    warmed.insert(our_id.to_string());
                }
            }
        }

        // Pass 2: for each credentialled provider, fetch from its own live
        // API.  Authoritative results overwrite the ModelDB-derived data.
        // Skip if the cache is already fresh (within 24 hours).
        let ttl = model_cache::FRESH_TTL_SECS;
        let credentials = crate::utils::providers::configured_providers(&creds);
        for (provider, _display) in &credentials {
            if provider.starts_with("custom:") {
                continue;
            }

            // Skip if cache is fresh
            if model_cache::is_fresh(provider, ttl) {
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

// ── ModelDB catalog ─────────────────────────────────────────────────────────

/// ModelDB provider_id → our internal provider ID.
const MDB_PROVIDER_MAP: &[(&str, &str)] = &[
    ("anthropic", "anthropic"),
    ("openai", "openai"),
    ("chatgpt", "openai"),
    ("google", "gemini"),
    ("minimax", "minimax"),
    ("dashscope", "qwen"),
    ("zai", "zhipu"),
    ("openrouter", "openrouter"),
    ("github", "github"),
    ("ollama", "ollama"),
    ("bedrock", "bedrock"),
    ("vertex", "vertex"),
];

/// The internal provider IDs we expect ModelDB to cover.
fn modeldb_known_ids() -> Vec<&'static str> {
    vec![
        "anthropic", "openai", "gemini", "minimax", "qwen", "zhipu",
        "openrouter", "github", "ollama", "bedrock", "vertex",
    ]
}

/// True when at least one known provider has a stale or missing cache entry,
/// meaning we should re-fetch from ModelDB.
fn needs_modeldb_refresh() -> bool {
    let ttl = model_cache::FRESH_TTL_SECS;
    !modeldb_known_ids()
        .into_iter()
        .all(|id| model_cache::is_fresh(id, ttl))
}

/// Fetch model IDs from ModelDB (https://modeldb.axiom.co) — a free, no-auth,
/// hourly-synced catalog built from LiteLLM's model pricing data.
///
/// Only returns entries for providers in [`modeldb_known_ids`].  Unknown
/// providers are silently dropped so the on-disk cache stays lean.
async fn fetch_modeldb_catalog() -> HashMap<String, Vec<String>> {
    let url = "https://modeldb.axiom.co/api/v1/models?project=model_id,provider_id";
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };

    let resp = match client.get(url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return HashMap::new(),
    };

    let entries: Vec<serde_json::Value> = match resp.json().await {
        Ok(v) => v,
        Err(_) => return HashMap::new(),
    };

    let overrides: HashMap<&str, &str> = MDB_PROVIDER_MAP.iter().copied().collect();
    let mut catalog: HashMap<String, Vec<String>> = HashMap::new();

    for entry in &entries {
        let model_id = match entry.get("model_id").and_then(|v| v.as_str()) {
            Some(id) if !id.is_empty() => id,
            _ => continue,
        };
        let raw_provider = match entry.get("provider_id").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p,
            _ => continue,
        };

        let our_id = match overrides.get(raw_provider).copied() {
            Some(id) => id,
            None => continue, // unknown provider, skip to keep cache lean
        };

        catalog
            .entry(our_id.to_string())
            .or_default()
            .push(model_id.to_string());
    }

    // Deduplicate and sort newest-first within each bucket
    for models in catalog.values_mut() {
        models.sort();
        models.dedup();
        models.reverse();
    }

    catalog
}
