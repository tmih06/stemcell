//! Anthropic (Claude) Provider Implementation using rig-core
//!
//! Implements the Provider trait for Anthropic's Claude models.
//! Uses rig-core as the backend engine.

use rig_core::providers::anthropic::Client;
use std::sync::Arc;

/// Anthropic provider for Claude models
#[derive(Clone)]
pub struct AnthropicProvider {
    _api_key: String,
    client: Client,
    custom_default_model: Option<String>,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider
    pub fn new(api_key: String) -> Self {
        // Note: Rig's Client does not expose prompt caching headers directly without extensions,
        // we will implement extensions later.

        Self {
            _api_key: api_key.clone(),
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

        crate::brain::provider::rig_adapter::RigAdapter {
            name: "anthropic".into(),
            default_model: self
                .custom_default_model
                .unwrap_or_else(|| "claude-sonnet-4-5".to_string()),
            supported_models: vec![
                "claude-opus-4-6".to_string(),
                "claude-sonnet-4-5-20250929".to_string(),
            ],
            context_window_fn: Some(Arc::new(|_m| Some(200_000))),
            calculate_cost_fn: None,
            base_url: None,
            client_builder: Arc::new(move || client.clone()),
        }
    }
}
