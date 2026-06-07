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
    custom_default_model: Option<String>,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider
    pub fn new(api_key: String) -> Self {
        // Note: Rig's Client does not expose prompt caching headers directly without extensions,
        // we will implement extensions later.

        Self {
            client: rig_core::providers::anthropic::Client::new(&api_key).unwrap(),
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

        crate::brain::provider::rig_adapter::RigAdapter {
            name: "anthropic".into(),
            default_model,
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
        }
    }
}
