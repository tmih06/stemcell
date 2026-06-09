//! Startup job: fetch every configured provider's model list and warm the
//! on-disk cache so the `/models` dialog opens instantly.
//!
//! Network-bound — exactly the kind of slow work that belongs in a background
//! startup job. On failure it logs and leaves any prior cache intact.

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

        // Collect providers to warm: start with those that have a key or
        // are always-available CLI providers, then augment with providers
        // that expose a public model-listing endpoint (no key required).
        let mut providers = crate::utils::providers::configured_providers(&creds);

        // OpenRouter's GET /api/v1/models is public — always warm it so
        // the model cache is populated even before the user adds a key.
        if !providers.iter().any(|(id, _)| id == "openrouter") {
            providers.push(("openrouter".to_string(), "OpenRouter".to_string()));
        }

        if providers.is_empty() {
            tracing::debug!("[startup] fetch-models: no configured providers, skipping");
            return Ok(Some("no configured providers".to_string()));
        }

        let mut warmed: Vec<String> = Vec::new();
        let mut total_models = 0usize;
        for (provider, _display) in providers {
            // Custom providers default to paste mode and never read the cache,
            // so warming them would be write-only.
            if provider.starts_with("custom:") {
                continue;
            }

            let Some(provider_index) = crate::utils::providers::tui_index_for_id(&provider) else {
                tracing::warn!("[startup] fetch-models: no TUI index for '{provider}', skipping");
                continue;
            };
            let api_key = crate::utils::providers::config_for(&creds, &provider)
                .and_then(|p| p.api_key.clone());

            let models = crate::tui::onboarding::fetch_provider_models(
                provider_index,
                api_key.as_deref(),
                None,
                None,
            )
            .await;

            if models.is_empty() {
                tracing::warn!(
                    "[startup] fetch-models: empty list for '{provider}', cache unchanged"
                );
                continue;
            }

            let count = models.len();
            model_cache::store(&provider, models);
            tracing::info!("[startup] fetch-models: cached {count} models for '{provider}'");
            total_models += count;
            warmed.push(provider);
        }

        if warmed.is_empty() {
            return Ok(Some("no model lists warmed".to_string()));
        }
        Ok(Some(format!(
            "cached {total_models} models for {} provider(s): {}",
            warmed.len(),
            warmed.join(", ")
        )))
    }
}
