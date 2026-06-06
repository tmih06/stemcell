//! Google Gemini Provider Implementation using rig-core
//!
//! Implements the Provider trait for Google's Gemini models.
//! Uses rig-core as the backend engine.

use super::error::{ProviderError, Result};
use super::r#trait::{Provider, ProviderStream};
use super::types::*;
use async_trait::async_trait;
use rig_core::providers::gemini::Client;
use rig_core::completion::{CompletionModel, CompletionRequest, Message as RigMessage};
use rig_core::client::CompletionClient;
use std::sync::Arc;

/// Google Gemini provider
#[derive(Clone)]
pub struct GeminiProvider {
    api_key: String,
    client: Client,
    model: String,
}

impl GeminiProvider {
    /// Create a new Gemini provider
    pub fn new(api_key: String) -> Self {
        let client = Client::new(&api_key).expect("Failed to initialize Rig Gemini client");

        Self {
            api_key,
            client,
            model: "gemini-2.0-flash".to_string(),
        }
    }

    /// Set the default model
    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    pub fn build(self) -> crate::brain::provider::rig_adapter::RigAdapter<Client> {
        let client = self.client.clone();
        
        crate::brain::provider::rig_adapter::RigAdapter {
            name: "gemini".into(),
            default_model: self.model,
            supported_models: vec![
                "gemini-2.0-flash".to_string(),
                "gemini-3.1-flash-image-preview".to_string(),
            ],
            context_window_fn: Some(Arc::new(|_m| Some(1_000_000))),
            calculate_cost_fn: None,
            base_url: None,
            client_builder: Arc::new(move || client.clone()),
        }
    }
}
