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

    async fn run(&self, ctx: &StartupContext) -> anyhow::Result<()> {
        let config = &ctx.config;

        // Warm every provider the user has a usable credential for, so switching
        // providers in the /models dialog is also instant — not just the active
        // one. configured_providers() already filters to credentialed providers.
        let providers = crate::utils::providers::configured_providers(&config.providers);
        if providers.is_empty() {
            tracing::debug!("[startup] fetch-models: no configured providers, skipping");
            return Ok(());
        }

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
            let api_key = crate::utils::providers::config_for(&config.providers, &provider)
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
        }

        Ok(())
    }
}
