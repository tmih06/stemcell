//! Anthropic (Claude) Provider Implementation using rig-core
//!
//! Implements the Provider trait for Anthropic's Claude models.
//! Uses rig-core as the backend engine.

use rig_core::providers::anthropic::Client;
use std::sync::Arc;

/// Anthropic provider for Claude models
#[derive(Clone)]
pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    custom_default_model: Option<String>,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider
    pub fn new(api_key: String) -> Self {
        // Note: Rig's Client does not expose prompt caching headers directly without extensions,
        // we will implement extensions later.

        Self {
            client: rig_core::providers::anthropic::Client::new(&api_key).unwrap(),
            api_key,
            custom_default_model: None,
        }
    }

    /// Set custom default model
    pub fn with_default_model(mut self, model: String) -> Self {
        self.custom_default_model = Some(model);
        self
    }

    pub fn build(self) -> crate::brain::provider::rig_adapter::RigAdapter<Client> {
        let client = self.client.clone();
        let default_model = self
            .custom_default_model
            .clone()
            .unwrap_or_else(|| "claude-sonnet-4-20250514".to_string());

        let api_key = self.api_key.clone();
        let fetch_models_fn: crate::brain::provider::rig_adapter::FetchModelsFn =
            Arc::new(move || {
                let api_key = api_key.clone();
                Box::pin(async move {
                    use crate::brain::provider::model_fetch::{
                        DEFAULT_MODEL_CACHE_TTL, cached_or_fetch, fetch_anthropic_models,
                    };
                    cached_or_fetch("anthropic", DEFAULT_MODEL_CACHE_TTL, || {
                        fetch_anthropic_models(Some(api_key.as_str()))
                    })
                    .await
                })
            });

        crate::brain::provider::rig_adapter::RigAdapter {
            name: "anthropic".into(),
            default_model,
            // Offline-fallback seed only. The live list comes from
            // `fetch_anthropic_models` via `fetch_models_fn`; this is what
            // `validate_model`/`supported_models` return when the network
            // is unavailable and the disk cache is cold.
            supported_models: vec![
                "claude-sonnet-4-20250514".to_string(),
                "claude-opus-4-1-20250805".to_string(),
                "claude-3-5-haiku-20241022".to_string(),
            ],
            context_window_fn: Some(Arc::new(|_m| Some(200_000))),
            calculate_cost_fn: None,
            base_url: None,
            client_builder: Arc::new(move || client.clone()),
            vision_model: None,
            fetch_models_fn: Some(fetch_models_fn),
        }
    }
}
