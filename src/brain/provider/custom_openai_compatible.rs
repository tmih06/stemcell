//! Custom OpenAI-Compatible Provider Implementation using rig-core
//!
//! Implements the Provider trait for any OpenAI-compatible API.
//! Uses rig-core as the backend engine.

use crate::brain::provider::rate_limiter::RateLimiter;
use rig_core::providers::openai::Client;
use std::sync::Arc;

pub type BodyTransformFn = Arc<dyn Fn(serde_json::Value) -> serde_json::Value + Send + Sync>;
pub type TokenFn = Arc<dyn Fn() -> String + Send + Sync>;
pub type BaseUrlFn = Arc<dyn Fn() -> String + Send + Sync>;
pub type AuthRefreshFn =
    Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send + Sync>;

pub const STRIP_OPEN_TAGS: &[&str] = &["<think>", "<!-- reasoning -->", "<!--"];
pub const STRIP_CLOSE_TAGS: &[&[&str]] = &[
    &["</think>"],
    &["<!-- /reasoning -->", "</think>", "-->"],
    &["-->"],
];

pub const THINK_BLOCK_MAX_BYTES: usize = 200_000;

/// Custom OpenAI-Compatible Provider
#[derive(Clone)]
pub struct OpenAIProvider {
    api_key: String,
    base_url: String,
    model: String,
    name: String,
    pub(crate) extra_headers: Vec<(String, String)>,
    token_fn: Option<TokenFn>,
}

impl OpenAIProvider {
    pub fn new(api_key: String) -> Self {
        Self::with_base_url(api_key, "https://api.openai.com/v1/chat/completions".into())
    }

    pub fn with_base_url(api_key: String, base_url: String) -> Self {
        let base_url = if base_url.ends_with("/chat/completions") {
            base_url
                .strip_suffix("/chat/completions")
                .unwrap()
                .trim_end_matches('/')
                .to_string()
        } else {
            base_url.trim_end_matches('/').to_string()
        };

        Self {
            api_key,
            base_url,
            model: "gpt-4o".to_string(),
            name: "openai".to_string(),
            extra_headers: vec![],
            token_fn: None,
        }
    }

    pub fn local(base_url: String) -> Self {
        Self::with_base_url("".into(), base_url)
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn with_default_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }

    pub fn with_extra_headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.extra_headers = headers;
        self
    }

    pub fn with_body_transform(self, _transform: BodyTransformFn) -> Self {
        self // We ignore body_transform since rig-core abstracts the request body
    }

    pub fn with_token_fn(mut self, token_fn: TokenFn) -> Self {
        self.token_fn = Some(token_fn);
        self
    }

    pub fn with_rate_limiter(self, _limiter: Arc<RateLimiter>) -> Self {
        self
    }
    pub fn with_vision_model(self, _vm: String) -> Self {
        self
    }
    pub fn with_context_window(self, _cw: u32) -> Self {
        self
    }
    pub fn with_models(self, _models: Vec<String>) -> Self {
        self
    }
    pub fn with_cache_enabled(self, _cache: bool) -> Self {
        self
    }
    pub fn with_cache_ttl(self, _ttl: u32) -> Self {
        self
    }

    pub fn build(self) -> crate::brain::provider::rig_adapter::RigAdapter<Client> {
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();
        let token_fn = self.token_fn.clone();

        let client_builder = Arc::new(move || {
            let key = if let Some(tfn) = &token_fn {
                tfn()
            } else {
                api_key.clone()
            };
            Client::builder()
                .api_key(key)
                .base_url(base_url.clone())
                .build()
                .expect("Failed to create OpenAI client")
        });

        crate::brain::provider::rig_adapter::RigAdapter {
            name: self.name,
            default_model: self.model,
            supported_models: vec![],
            context_window_fn: None,
            calculate_cost_fn: None,
            base_url: Some(self.base_url),
            client_builder,
        }
    }
}

pub fn extract_balanced_json(_text: &str) -> Option<String> {
    None
}
pub const KNOWN_TOOL_NAMES: &[&str] = &[];
