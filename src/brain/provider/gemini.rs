//! Google Gemini Provider Implementation using rig-core
//!
//! Implements the Provider trait for Google's Gemini models.
//! Uses rig-core as the backend engine.

use rig_core::providers::gemini::Client;
use serde_json::Value;
use std::sync::Arc;

/// Recursively strip `additionalProperties`, `default`, and `example` keys
/// from a JSON Schema before sending it to Gemini's
/// `function_declarations[].parameters` validator. Issue #99: Gemini's
/// validator rejects those keys. The rest of the schema is preserved.
pub fn sanitize_schema_for_gemini(schema: Value) -> Value {
    match schema {
        Value::Object(mut map) => {
            map.remove("additionalProperties");
            map.remove("default");
            map.remove("example");
            let mut new_map = serde_json::Map::new();
            for (k, v) in map {
                new_map.insert(k, sanitize_schema_for_gemini(v));
            }
            Value::Object(new_map)
        }
        Value::Array(arr) => {
            Value::Array(arr.into_iter().map(sanitize_schema_for_gemini).collect())
        }
        other => other,
    }
}

/// Google Gemini provider
#[derive(Clone)]
pub struct GeminiProvider {
    client: Client,
    model: String,
}

impl GeminiProvider {
    /// Create a new Gemini provider
    pub fn new(api_key: String) -> Self {
        let client = Client::new(&api_key).expect("Failed to initialize Rig Gemini client");

        Self {
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
            vision_model: None,
        }
    }
}
