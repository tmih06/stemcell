//! Startup job: fetch the active provider's model list and warm the on-disk
//! cache so the `/models` dialog opens instantly.
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
        let (provider, _model) = config.providers.active_provider_and_model();

        if provider == "none" {
            tracing::debug!("[startup] fetch-models: no active provider, skipping");
            return Ok(());
        }

        // Only built-in providers are warm-started: the /models dialog reads the
        // cache solely for built-ins (custom providers default to paste mode and
        // never read it), so caching custom entries would be write-only.
        if provider.starts_with("custom:") {
            tracing::debug!("[startup] fetch-models: custom provider, skipping cache warm");
            return Ok(());
        }

        let provider_index = crate::utils::providers::tui_index_for_id(&provider)
            .ok_or_else(|| anyhow::anyhow!("no TUI index for provider '{provider}'"))?;
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
            tracing::warn!("[startup] fetch-models: empty list for '{provider}', cache unchanged");
            return Ok(());
        }

        let count = models.len();
        model_cache::store(&provider, models);
        tracing::info!("[startup] fetch-models: cached {count} models for '{provider}'");
        Ok(())
    }
}
