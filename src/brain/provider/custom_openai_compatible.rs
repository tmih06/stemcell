//! Custom OpenAI-Compatible Provider Implementation using rig-core
//!
//! Implements the Provider trait for any OpenAI-compatible API.
//! Uses rig-core as the backend engine.
//!
//! The legacy request-encoding surface (`to_openai_request`, error
//! unwrapping, kimi `reasoning_content` injection, etc.) is kept around
//! for the regression tests in `src/tests/kimi_reasoning_test.rs` and
//! `src/tests/provider_error_proxy_test.rs` so the rig-core migration
//! doesn't lose the contracts those tests pin.

use crate::brain::provider::rate_limiter::RateLimiter;
use rig_core::providers::openai::CompletionsClient;
use serde::{Deserialize, Serialize};
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
    /// When set, swap to this model for requests containing images.
    vision_model: Option<String>,
    /// Configured context window size (overrides model-name heuristics).
    configured_context_window: Option<u32>,
    /// Models advertised by this provider.
    configured_models: Vec<String>,
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

        let is_openai_default = base_url.starts_with("https://api.openai.com/v1")
            || base_url == "https://api.openai.com/v1";
        let name = if is_openai_default {
            "openai".to_string()
        } else {
            "openai-compatible".to_string()
        };

        Self {
            api_key,
            base_url,
            model: "gpt-4o".to_string(),
            name,
            extra_headers: vec![],
            token_fn: None,
            vision_model: None,
            configured_context_window: None,
            configured_models: Vec::new(),
        }
    }

    pub fn local(base_url: String) -> Self {
        Self::with_base_url("".into(), base_url).with_name("openai-compatible")
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
        self
    }

    pub fn with_token_fn(mut self, token_fn: TokenFn) -> Self {
        self.token_fn = Some(token_fn);
        self
    }

    pub fn with_rate_limiter(self, _limiter: Arc<RateLimiter>) -> Self {
        self
    }
    pub fn with_vision_model(mut self, vm: String) -> Self {
        self.vision_model = Some(vm);
        self
    }
    pub fn with_context_window(mut self, cw: u32) -> Self {
        self.configured_context_window = Some(cw);
        self
    }
    pub fn with_models(mut self, models: Vec<String>) -> Self {
        self.configured_models = models;
        self
    }
    pub fn with_cache_enabled(self, _cache: bool) -> Self {
        self
    }
    pub fn with_cache_ttl(self, _ttl: u32) -> Self {
        self
    }

    /// Get the configured provider name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the configured default model.
    pub fn default_model(&self) -> &str {
        &self.model
    }

    /// Get the configured supported models list, falling back to the
    /// OpenAI model family when nothing was explicitly configured.
    pub fn supported_models(&self) -> Vec<String> {
        if !self.configured_models.is_empty() {
            return self.configured_models.clone();
        }
        vec![
            "gpt-4".to_string(),
            "gpt-4-turbo-preview".to_string(),
            "gpt-4o".to_string(),
            "gpt-4o-mini".to_string(),
            "gpt-3.5-turbo".to_string(),
        ]
    }

    /// Context window for `model`. Uses the explicit override when set;
    /// otherwise falls back to the model-name heuristic.
    pub fn context_window(&self, model: &str) -> Option<u32> {
        if let Some(cw) = self.configured_context_window {
            return Some(cw);
        }
        context_window_for_model(model)
    }

    /// Cost calculation. Pulls from the user's `usage_pricing.toml`
    /// (with the embedded example as fallback) so cost is consistent
    /// across all callers — the TUI footer, `/usage`, and the Provider
    /// trait. Returns 0 when the model isn't in the pricing table.
    pub fn calculate_cost(&self, model: &str, input_tokens: u32, output_tokens: u32) -> f64 {
        crate::usage::pricing::PricingConfig::load()
            .map(|cfg| cfg.calculate_cost(model, input_tokens, output_tokens))
            .unwrap_or(0.0)
    }

    /// Get the configured vision model name (if any).
    pub fn vision_model(&self) -> Option<&str> {
        self.vision_model.as_deref()
    }

    /// Whether this provider has a vision backend configured.
    pub fn supports_vision(&self) -> bool {
        self.vision_model.is_some()
    }

    pub fn build(self) -> crate::brain::provider::rig_adapter::RigAdapter<CompletionsClient> {
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();
        let token_fn = self.token_fn.clone();
        let configured_cw = self.configured_context_window;

        let client_builder = Arc::new(move || {
            let key = if let Some(tfn) = &token_fn {
                tfn()
            } else {
                api_key.clone()
            };
            CompletionsClient::builder()
                .api_key(key)
                .base_url(base_url.clone())
                .build()
                .expect("Failed to create OpenAI client")
        });

        let context_window_fn: Option<Arc<dyn Fn(&str) -> Option<u32> + Send + Sync>> =
            configured_cw.map(|cw| -> Arc<dyn Fn(&str) -> Option<u32> + Send + Sync> { Arc::new(move |_model| Some(cw)) });

        let calculate_cost_fn: Option<Arc<dyn Fn(&str, u32, u32) -> f64 + Send + Sync>> =
            Some(Arc::new(
                move |model, input_tokens, output_tokens| {
                    crate::usage::pricing::PricingConfig::load()
                        .map(|cfg| cfg.calculate_cost(model, input_tokens, output_tokens))
                        .unwrap_or(0.0)
                },
            ));

        crate::brain::provider::rig_adapter::RigAdapter {
            name: self.name,
            default_model: self.model,
            supported_models: self.configured_models,
            context_window_fn,
            calculate_cost_fn,
            base_url: Some(self.base_url),
            client_builder,
            vision_model: self.vision_model,
        }
    }

    /// Convert our generic request to OpenAI-specific format.
    ///
    /// Kept verbatim from the pre-migration provider so the kimi reasoning
    /// tests (`kimi_reasoning_test.rs`) can pin the encoding. The
    /// rig-core primary path does NOT go through this — `RigAdapter`
    /// builds its own `CompletionRequest`. This method exists only for
    /// the test surface.
    pub fn to_openai_request(&self, request: crate::brain::provider::types::LLMRequest) -> OpenAIRequest {
        use crate::brain::provider::types::{ContentBlock, ImageSource, Role};

        let mut messages = Vec::new();

        if let Some(system) = request.system {
            messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: Some(serde_json::Value::String(system)),
                tool_calls: None,
                tool_call_id: None,
                reasoning_content: None,
            });
        }

        let needs_reasoning_content = needs_reasoning_content_for(&self.base_url, &request.model);

        for msg in request.messages {
            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::System => "system",
            };

            let mut text_parts = Vec::new();
            let mut image_parts: Vec<serde_json::Value> = Vec::new();
            let mut tool_uses = Vec::new();
            let mut tool_results = Vec::new();
            let mut thinking_parts: Vec<String> = Vec::new();

            for block in msg.content {
                match block {
                    ContentBlock::Text { text } => text_parts.push(text),
                    ContentBlock::ToolUse { id, name, input } => {
                        tool_uses.push((id, name, input));
                    }
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        ..
                    } => {
                        tool_results.push((tool_use_id, content));
                    }
                    ContentBlock::Thinking { thinking, .. } => {
                        if !thinking.is_empty() {
                            thinking_parts.push(thinking);
                        }
                    }
                    ContentBlock::Image { source } => {
                        let url = match source {
                            ImageSource::Base64 { media_type, data } => {
                                format!("data:{};base64,{}", media_type, data)
                            }
                            ImageSource::Url { url } => url,
                        };
                        image_parts.push(serde_json::json!({
                            "type": "image_url",
                            "image_url": { "url": url }
                        }));
                    }
                }
            }

            let make_content =
                |texts: &[String], images: &[serde_json::Value]| -> Option<serde_json::Value> {
                    if !images.is_empty() {
                        let mut parts: Vec<serde_json::Value> = Vec::new();
                        if !texts.is_empty() {
                            parts.push(serde_json::json!({
                                "type": "text",
                                "text": texts.join("\n")
                            }));
                        }
                        parts.extend(images.iter().cloned());
                        Some(serde_json::Value::Array(parts))
                    } else if !texts.is_empty() {
                        Some(serde_json::Value::String(texts.join("\n")))
                    } else {
                        None
                    }
                };

            if !tool_uses.is_empty() {
                let openai_tool_calls = tool_uses
                    .into_iter()
                    .map(|(id, name, input)| OpenAIToolCall {
                        id,
                        r#type: "function".to_string(),
                        function: OpenAIFunctionCall {
                            name,
                            arguments: serde_json::to_string(&input).unwrap_or_default(),
                        },
                    })
                    .collect();

                let content_val = make_content(&text_parts, &image_parts);
                let reasoning_content = if !thinking_parts.is_empty() {
                    Some(thinking_parts.join("\n"))
                } else if needs_reasoning_content {
                    Some(" ".to_string())
                } else {
                    None
                };

                messages.push(OpenAIMessage {
                    role: role.to_string(),
                    content: content_val,
                    tool_calls: Some(openai_tool_calls),
                    tool_call_id: None,
                    reasoning_content,
                });
            } else if !tool_results.is_empty() {
                for (tool_use_id, content) in tool_results {
                    messages.push(OpenAIMessage {
                        role: "tool".to_string(),
                        content: Some(serde_json::Value::String(content)),
                        tool_calls: None,
                        tool_call_id: Some(tool_use_id),
                        reasoning_content: None,
                    });
                }
            } else {
                let content_val = make_content(&text_parts, &image_parts)
                    .unwrap_or(serde_json::Value::String(String::new()));
                let reasoning_content = if role == "assistant" && !thinking_parts.is_empty() {
                    Some(thinking_parts.join("\n"))
                } else {
                    None
                };

                messages.push(OpenAIMessage {
                    role: role.to_string(),
                    content: Some(content_val),
                    tool_calls: None,
                    tool_call_id: None,
                    reasoning_content,
                });
            }
        }

        let tools: Option<Vec<OpenAITool>> = request.tools.map(|tools| {
            tools
                .iter()
                .map(|tool| OpenAITool {
                    r#type: "function".to_string(),
                    function: OpenAIFunction {
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        parameters: tool.input_schema.clone(),
                    },
                })
                .collect()
        });

        let uses_completion_tokens = uses_max_completion_tokens(&request.model);
        let (max_tokens, max_completion_tokens) = if uses_completion_tokens {
            (None, request.max_tokens)
        } else {
            (request.max_tokens, None)
        };

        let tool_choice = tools
            .as_ref()
            .filter(|t| !t.is_empty())
            .map(|_| serde_json::json!("auto"));

        let base = self.base_url.to_lowercase();
        let include_reasoning = if base.contains("openrouter")
            || base.contains("openrouter.ai")
            || std::env::var("OPENCRABS_ENABLE_REASONING").is_ok()
        {
            Some(true)
        } else {
            None
        };

        OpenAIRequest {
            model: request.model,
            messages,
            temperature: request.temperature,
            max_tokens,
            max_completion_tokens,
            stream: Some(request.stream),
            stream_options: None,
            tools,
            tool_choice,
            include_reasoning,
        }
    }
}

fn context_window_for_model(model: &str) -> Option<u32> {
    let m = model.to_ascii_lowercase();
    if m.starts_with("gpt-4.1") {
        Some(1_047_576)
    } else if m.starts_with("gpt-5") {
        Some(1_047_576)
    } else if m == "gpt-4o" || m == "gpt-4o-mini" || m == "gpt-4-turbo-preview" {
        Some(128_000)
    } else if m == "gpt-4" {
        Some(8_192)
    } else if m == "gpt-3.5-turbo" {
        Some(4_096)
    } else if m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") {
        Some(200_000)
    } else {
        None
    }
}

// ============================================================================
// OpenAI-compatible request/response types (kept for the test surface)
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct OpenAIRequest {
    pub model: String,
    pub messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    include_reasoning: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Thinking / reasoning text associated with an assistant message.
    /// Moonshot kimi (direct and via opencode.ai/zen/go) 400s on tool-call
    /// messages that omit this when thinking mode is on upstream.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolCall {
    pub id: String,
    pub r#type: String,
    pub function: OpenAIFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
struct OpenAITool {
    r#type: String,
    function: OpenAIFunction,
}

#[derive(Debug, Clone, Serialize)]
struct OpenAIFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
struct StreamOptions {
    include_usage: bool,
}

// ============================================================================
// Token field routing + error unwrapping
// ============================================================================

/// Returns true if this model requires `max_completion_tokens` instead of `max_tokens`.
/// Newer OpenAI models (gpt-4.1-*, gpt-5-*, o1-*, o3-*) reject `max_tokens`.
/// Qwen thinking models also need this.
pub fn uses_max_completion_tokens(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    m.starts_with("gpt-4.1")
        || m.starts_with("gpt-5")
        || m.starts_with("o1")
        || m.starts_with("o3")
        || m.starts_with("o4")
        || m.contains("thinking")
}

/// Returns true if the error message indicates a max_tokens / max_completion_tokens mismatch.
pub fn is_token_field_mismatch(msg: &str) -> bool {
    let m = msg.to_lowercase();
    (m.contains("max_tokens") || m.contains("max_completion_tokens")) && m.contains("unsupported")
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAIErrorResponse {
    pub error: OpenAIError,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAIError {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: Option<String>,
    /// Proxy-wrapped upstream errors. opencode.ai in particular hides the
    /// real backend error inside `error.metadata.raw` as a stringified
    /// JSON blob.
    #[serde(default)]
    pub metadata: Option<OpenAIErrorMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenAIErrorMetadata {
    #[serde(default)]
    pub raw: Option<String>,
    #[serde(default)]
    pub provider_name: Option<String>,
}

/// True when an OpenAI-compatible endpoint routes to Moonshot kimi
/// and therefore requires `reasoning_content` on assistant tool-call
/// messages. Matches both the direct Moonshot URL and opencode.ai's
/// zen/go proxy when the model name contains "kimi".
pub fn needs_reasoning_content_for(base_url: &str, model: &str) -> bool {
    let url = base_url.to_ascii_lowercase();
    let model = model.to_ascii_lowercase();
    url.contains("moonshot") || (url.contains("opencode.ai") && model.contains("kimi"))
}

/// Walk a proxy's nested error envelope to find the real upstream error.
/// Returns (message, error_type) after stripping the passthrough wrapper.
pub fn unwrap_proxy_error(outer: &OpenAIError) -> (String, Option<String>) {
    let Some(ref metadata) = outer.metadata else {
        return (outer.message.clone(), outer.error_type.clone());
    };
    let Some(ref raw) = metadata.raw else {
        return (outer.message.clone(), outer.error_type.clone());
    };
    let Ok(inner) = serde_json::from_str::<OpenAIErrorResponse>(raw) else {
        let prefix = metadata
            .provider_name
            .as_deref()
            .map(|p| format!("[{}] ", p))
            .unwrap_or_default();
        return (
            format!("{}{}: {}", prefix, outer.message, raw),
            outer.error_type.clone(),
        );
    };
    let prefix = metadata
        .provider_name
        .as_deref()
        .map(|p| format!("[{}] ", p))
        .unwrap_or_default();
    (
        format!("{}{}", prefix, inner.error.message),
        inner
            .error
            .error_type
            .clone()
            .or_else(|| outer.error_type.clone()),
    )
}

// ============================================================================
// Qwen-style text tool-call extraction
// ============================================================================

/// Tool names we recognise when the model emits Claude-style native XML
/// invocations.
pub const KNOWN_TOOL_NAMES: &[&str] = &[
    "bash",
    "ls",
    "glob",
    "grep",
    "read_file",
    "write_file",
    "edit_file",
    "patch_file",
    "web_search",
    "web_fetch",
    "web_request",
    "http_request",
    "plan",
    "task_manager",
    "cron_manage",
    "memory_search",
    "session_search",
    "lsp",
    "agent",
    "slack_send",
    "telegram_send",
    "discord_send",
    "trello_send",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BareToolArrayMatch {
    Full,
    Prefix,
    None,
}

pub fn classify_bare_tool_array(s: &str) -> BareToolArrayMatch {
    fn step<'a>(t: &'a str, literal: &str, state: &mut BareToolArrayMatch) -> Option<&'a str> {
        let t = t.trim_start();
        if t.is_empty() {
            *state = BareToolArrayMatch::Prefix;
            return None;
        }
        if t.len() < literal.len() {
            *state = if literal.starts_with(t) {
                BareToolArrayMatch::Prefix
            } else {
                BareToolArrayMatch::None
            };
            return None;
        }
        if let Some(rest) = t.strip_prefix(literal) {
            Some(rest)
        } else {
            *state = BareToolArrayMatch::None;
            None
        }
    }

    let t = s.trim_start();
    if t.is_empty() {
        return BareToolArrayMatch::Prefix;
    }
    let mut state = BareToolArrayMatch::None;
    let Some(t) = step(t, "[", &mut state) else {
        return state;
    };
    let Some(t) = step(t, "{", &mut state) else {
        return state;
    };
    let Some(t) = step(t, "\"id\"", &mut state) else {
        return state;
    };
    let Some(t) = step(t, ":", &mut state) else {
        return state;
    };
    let Some(_) = step(t, "\"call_", &mut state) else {
        return state;
    };
    BareToolArrayMatch::Full
}

/// Consume a balanced JSON object or array starting at `s[0]`. Returns
/// the byte length through the matching closing `}`, or `None` if
/// unbalanced.
pub fn extract_balanced_json(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let (open, close) = match bytes.first()? {
        b'{' => (b'{', b'}'),
        b'[' => (b'[', b']'),
        _ => return None,
    };
    let mut depth: i32 = 0;
    let mut in_string = false;
    let mut escape = false;
    for (idx, &b) in bytes.iter().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            match b {
                b'\\' => escape = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        if b == b'"' {
            in_string = true;
        } else if b == open {
            depth += 1;
        } else if b == close {
            depth -= 1;
            if depth == 0 {
                return Some(idx + 1);
            }
        }
    }
    None
}

fn parse_qwen_tool_json(json: &str) -> Option<(String, serde_json::Value)> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    parse_tool_call_value(&v)
}

fn parse_tool_call_value(v: &serde_json::Value) -> Option<(String, serde_json::Value)> {
    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .or_else(|| v.get("tool_name").and_then(|n| n.as_str()))
        .or_else(|| {
            v.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
        })
        .or_else(|| {
            v.get("function")
                .and_then(|f| if f.is_string() { f.as_str() } else { None })
        })?
        .to_string();
    if name.is_empty() {
        return None;
    }
    let args_val = v
        .get("arguments")
        .or_else(|| v.get("args"))
        .or_else(|| v.get("input"))
        .or_else(|| v.get("parameters"))
        .or_else(|| v.get("function").and_then(|f| f.get("arguments")))
        .or_else(|| v.get("function").and_then(|f| f.get("parameters")));
    let input = match args_val {
        Some(serde_json::Value::String(s)) => {
            serde_json::from_str(s).unwrap_or(serde_json::json!({}))
        }
        Some(other) => other.clone(),
        None => serde_json::json!({}),
    };
    Some((name, input))
}

/// Extract tool_call blocks emitted as text content. Local GGUF/MLX
/// backends serving reasoning models will often put tool calls into
/// `message.content` instead of the structured `tool_calls` field. This
/// handles several on-the-wire shapes:
///   1. `<tool_call>{...}</tool_call>` — Qwen XML
///   2. `<function=name>...</function>` — Qwen v2 XML
///   3. `tool_call:{...}` — bare OpenAI envelope
///   4. `{"tool_calls":[{...}, ...]}` — multi-call envelope
///   5. `[{"id":"call_..."}]` — bare top-level array
///   6. `{"call_xxx":{"name":...,"arguments":...}}` — dict-by-call-id
///   7. `{"command": "..."}` — bare bash args
///   8. `<invoke name="X">...</invoke>` — Anthropic-style XML
pub fn extract_text_tool_calls(text: &str) -> (Vec<(String, serde_json::Value)>, String) {
    let has_claude_style = KNOWN_TOOL_NAMES
        .iter()
        .any(|t| text.contains(&format!("<{}>", t)));
    let has_bare_array_signal =
        text.contains("\"id\":\"call_") || text.contains("\"id\": \"call_");
    let has_dict_by_id_signal =
        text.contains("\"call_") && (text.contains("\"name\"") || text.contains("\"function\""));
    let has_bare_command_args = text.contains("{\"command\":")
        || text.contains("{ \"command\":")
        || text.contains("{\"command\" :");
    let has_invoke_signal =
        text.contains("<invoke name=") || text.contains("invoke name=\"");
    let has_bare_name_args = super::bare_tool_call_extractor::has_bare_name_args_signal(text);
    if !text.contains("<tool_call>")
        && !text.contains("<function=")
        && !text.contains("tool_call:")
        && !text.contains("\"tool_calls\"")
        && !text.contains("\"tool_call\"")
        && !has_claude_style
        && !has_bare_array_signal
        && !has_dict_by_id_signal
        && !has_bare_command_args
        && !has_invoke_signal
        && !has_bare_name_args
    {
        return (Vec::new(), text.to_string());
    }

    let mut tool_calls: Vec<(String, serde_json::Value)> = Vec::new();
    let mut strip_ranges: Vec<(usize, usize)> = Vec::new();

    if has_claude_style {
        for (start, end, name, input) in extract_claude_style_tool_calls(text) {
            tool_calls.push((name, input));
            strip_ranges.push((start, end));
        }
    }

    if has_bare_array_signal {
        let anchors = ["\"id\":\"call_", "\"id\": \"call_"];
        let mut search_from = 0;
        loop {
            let next = anchors
                .iter()
                .filter_map(|a| text[search_from..].find(a).map(|p| (search_from + p, *a)))
                .min_by_key(|(p, _)| *p);
            let Some((anchor_pos, anchor_lit)) = next else {
                break;
            };
            let window_start = anchor_pos.saturating_sub(64);
            let bracket_pos = text[window_start..anchor_pos]
                .rfind('[')
                .map(|r| window_start + r);
            let Some(arr_pos) = bracket_pos else {
                search_from = anchor_pos + anchor_lit.len();
                continue;
            };
            if strip_ranges
                .iter()
                .any(|(s, e)| *s <= arr_pos && arr_pos < *e)
            {
                search_from = anchor_pos + anchor_lit.len();
                continue;
            }
            match extract_balanced_json(&text[arr_pos..]) {
                Some(consumed) => {
                    let arr_slice = &text[arr_pos..arr_pos + consumed];
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(arr_slice)
                        && let Some(items) = v.as_array()
                    {
                        let mut found_any = false;
                        for item in items {
                            if let Some(call) = parse_tool_call_value(item) {
                                tool_calls.push(call);
                                found_any = true;
                            }
                        }
                        if found_any {
                            strip_ranges.push((arr_pos, arr_pos + consumed));
                            search_from = arr_pos + consumed;
                            continue;
                        }
                    }
                    search_from = anchor_pos + anchor_lit.len();
                }
                None => {
                    search_from = anchor_pos + anchor_lit.len();
                }
            }
        }
    }

    if has_dict_by_id_signal {
        let mut search_from = 0;
        while let Some(rel) = text[search_from..].find("\"call_") {
            let anchor_pos = search_from + rel;
            let mut back = anchor_pos;
            while back > 0 {
                let b = text.as_bytes()[back - 1];
                if b.is_ascii_whitespace() || b == b'\n' || b == b'\r' {
                    back -= 1;
                    continue;
                }
                break;
            }
            if back == 0 || text.as_bytes()[back - 1] != b'{' {
                search_from = anchor_pos + "\"call_".len();
                continue;
            }
            let obj_start = back - 1;
            if strip_ranges
                .iter()
                .any(|(s, e)| *s <= obj_start && obj_start < *e)
            {
                search_from = anchor_pos + "\"call_".len();
                continue;
            }
            let Some(consumed) = extract_balanced_json(&text[obj_start..]) else {
                search_from = anchor_pos + "\"call_".len();
                continue;
            };
            let obj_slice = &text[obj_start..obj_start + consumed];
            let Ok(v) = serde_json::from_str::<serde_json::Value>(obj_slice) else {
                search_from = anchor_pos + "\"call_".len();
                continue;
            };
            let Some(obj) = v.as_object() else {
                search_from = anchor_pos + "\"call_".len();
                continue;
            };
            let mut found_any = false;
            for (key, val) in obj {
                if !key.starts_with("call_") {
                    continue;
                }
                if let Some(call) = parse_tool_call_value(val) {
                    tool_calls.push(call);
                    found_any = true;
                }
            }
            if found_any {
                strip_ranges.push((obj_start, obj_start + consumed));
                search_from = obj_start + consumed;
            } else {
                search_from = anchor_pos + "\"call_".len();
            }
        }
    }

    let mut i: usize = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        let tc_at = text[i..].find("<tool_call>").map(|r| i + r);
        let fn_at = text[i..].find("<function=").map(|r| i + r);
        let bare_at = text[i..].find("tool_call:").map(|r| i + r);
        let arr_at = text[i..].find("\"tool_calls\"").map(|r| i + r);
        let sing_at = {
            let candidate = text[i..].find("\"tool_call\"").map(|r| i + r);
            match candidate {
                Some(p)
                    if text.as_bytes().get(p + "\"tool_call\"".len() - 1).copied()
                        != Some(b'"') =>
                {
                    None
                }
                Some(p) if text[p..].starts_with("\"tool_calls\"") => None,
                other => other,
            }
        };
        let next = [tc_at, fn_at, bare_at, arr_at, sing_at]
            .into_iter()
            .flatten()
            .min();
        let Some(start) = next else { break };

        if bare_at == Some(start) {
            if start > 0 {
                let prev = text.as_bytes()[start - 1];
                let is_boundary = prev.is_ascii_whitespace()
                    || matches!(
                        prev,
                        b',' | b';' | b':' | b'[' | b'(' | b'{' | b'\n' | b'\r'
                    );
                if !is_boundary {
                    i = start + "tool_call:".len();
                    continue;
                }
            }
            let body_start = start + "tool_call:".len();
            let brace_rel = text[body_start..]
                .char_indices()
                .find(|(_, c)| !c.is_whitespace())
                .map(|(idx, _)| idx);
            let brace_abs = match brace_rel {
                Some(rel) if text.as_bytes().get(body_start + rel) == Some(&b'{') => {
                    body_start + rel
                }
                _ => {
                    i = body_start;
                    continue;
                }
            };
            match extract_balanced_json(&text[brace_abs..]) {
                Some(consumed) => {
                    let json_slice = &text[brace_abs..brace_abs + consumed];
                    if let Some(call) = parse_qwen_tool_json(json_slice) {
                        tool_calls.push(call);
                        strip_ranges.push((start, brace_abs + consumed));
                        i = brace_abs + consumed;
                        continue;
                    }
                    i = body_start;
                }
                None => {
                    i = body_start;
                }
            }
            continue;
        } else if arr_at == Some(start) {
            let wrapper = text[..start].rfind('{');
            let wrapper_start = match wrapper {
                Some(br) if start - br <= 4 => br,
                _ => {
                    i = start + "\"tool_calls\"".len();
                    continue;
                }
            };
            match extract_balanced_json(&text[wrapper_start..]) {
                Some(consumed) => {
                    let env_slice = &text[wrapper_start..wrapper_start + consumed];
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(env_slice)
                        && let Some(arr) = v.get("tool_calls").and_then(|a| a.as_array())
                    {
                        let mut found_any = false;
                        for item in arr {
                            if let Some(call) = parse_tool_call_value(item) {
                                tool_calls.push(call);
                                found_any = true;
                            }
                        }
                        if found_any {
                            strip_ranges.push((wrapper_start, wrapper_start + consumed));
                            i = wrapper_start + consumed;
                            continue;
                        }
                    }
                    i = start + "\"tool_calls\"".len();
                }
                None => {
                    i = start + "\"tool_calls\"".len();
                }
            }
            continue;
        } else if sing_at == Some(start) {
            let wrapper = text[..start].rfind('{');
            let wrapper_start = match wrapper {
                Some(br) if start - br <= 4 => br,
                _ => {
                    i = start + "\"tool_call\"".len();
                    continue;
                }
            };
            match extract_balanced_json(&text[wrapper_start..]) {
                Some(consumed) => {
                    let env_slice = &text[wrapper_start..wrapper_start + consumed];
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(env_slice) {
                        let inner = v
                            .get("tool_call")
                            .or_else(|| v.get("function"))
                            .cloned()
                            .unwrap_or(v);
                        if let Some(call) = parse_tool_call_value(&inner) {
                            tool_calls.push(call);
                            strip_ranges.push((wrapper_start, wrapper_start + consumed));
                            i = wrapper_start + consumed;
                            continue;
                        }
                    }
                    // Strict JSON failed — try malformed-envelope recovery
                    // (Qwen sometimes drops the `:` after keys).
                    if let Some((name, args)) = parse_malformed_singular_envelope(env_slice) {
                        tool_calls.push((name, args));
                        strip_ranges.push((wrapper_start, wrapper_start + consumed));
                        i = wrapper_start + consumed;
                        continue;
                    }
                    i = start + "\"tool_call\"".len();
                }
                None => {
                    i = start + "\"tool_call\"".len();
                }
            }
            continue;
        } else if tc_at == Some(start) {
            let body_start = start + "<tool_call>".len();
            let brace_rel = text[body_start..]
                .char_indices()
                .find(|(_, c)| !c.is_whitespace())
                .map(|(idx, _)| idx);
            let brace_abs = match brace_rel {
                Some(rel) if text.as_bytes().get(body_start + rel) == Some(&b'{') => {
                    body_start + rel
                }
                _ => {
                    i = body_start;
                    continue;
                }
            };
            match extract_balanced_json(&text[brace_abs..]) {
                Some(consumed) => {
                    let json_slice = &text[brace_abs..brace_abs + consumed];
                    if let Some(call) = parse_qwen_tool_json(json_slice) {
                        tool_calls.push(call);
                    }
                    let mut end = brace_abs + consumed;
                    let after = &text[end..];
                    let ws_len = after.len() - after.trim_start().len();
                    if after.trim_start().starts_with("</tool_call>") {
                        end += ws_len + "</tool_call>".len();
                    }
                    strip_ranges.push((start, end));
                    i = end;
                }
                None => {
                    i = body_start;
                }
            }
        } else {
            let tag_start = start;
            let after = &text[tag_start..];
            let open_end = match after.find('>') {
                Some(r) => tag_start + r + 1,
                None => {
                    i = tag_start + "<function=".len();
                    continue;
                }
            };
            let name = text[tag_start + "<function=".len()..open_end - 1].trim();
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                i = open_end;
                continue;
            }
            let tail = &text[open_end..];
            let candidates = [
                tail.find("</tool_call>").map(|r| (r, "</tool_call>".len())),
                tail.find("<function=").map(|r| (r, 0usize)),
                tail.find("</function>").map(|r| (r, "</function>".len())),
            ];
            let pick = candidates.iter().filter_map(|o| *o).min_by_key(|(r, _)| *r);
            let (body_rel, close_len) = match pick {
                Some(p) => p,
                None => (tail.len(), 0),
            };
            let body = &tail[..body_rel];
            let input = parse_function_params(body);
            tool_calls.push((name.to_string(), input));
            let end = open_end + body_rel + close_len;
            strip_ranges.push((start, end));
            i = end;
        }
    }

    if has_bare_command_args {
        let mut search_from = 0;
        while let Some(rel) = text[search_from..].find("\"command\"") {
            let anchor_pos = search_from + rel;
            let mut back = anchor_pos;
            while back > 0 {
                let b = text.as_bytes()[back - 1];
                if b.is_ascii_whitespace() || b == b'\n' || b == b'\r' {
                    back -= 1;
                    continue;
                }
                break;
            }
            if back == 0 || text.as_bytes()[back - 1] != b'{' {
                search_from = anchor_pos + "\"command\"".len();
                continue;
            }
            let obj_start = back - 1;
            if strip_ranges
                .iter()
                .any(|(s, e)| *s <= obj_start && obj_start < *e)
            {
                search_from = anchor_pos + "\"command\"".len();
                continue;
            }
            let mut prev = obj_start;
            while prev > 0 {
                let b = text.as_bytes()[prev - 1];
                if b.is_ascii_whitespace() || b == b'\n' || b == b'\r' {
                    prev -= 1;
                    continue;
                }
                break;
            }
            if prev > 0 && text.as_bytes()[prev - 1] == b':' {
                let mut k = prev - 1;
                while k > 0 {
                    let b = text.as_bytes()[k - 1];
                    if b.is_ascii_whitespace() {
                        k -= 1;
                        continue;
                    }
                    break;
                }
                if k > 0 && text.as_bytes()[k - 1] == b'"' {
                    search_from = anchor_pos + "\"command\"".len();
                    continue;
                }
            }
            let Some(consumed) = extract_balanced_json(&text[obj_start..]) else {
                search_from = anchor_pos + "\"command\"".len();
                continue;
            };
            let obj_slice = &text[obj_start..obj_start + consumed];
            let Ok(v) = serde_json::from_str::<serde_json::Value>(obj_slice) else {
                search_from = anchor_pos + "\"command\"".len();
                continue;
            };
            let Some(obj) = v.as_object() else {
                search_from = anchor_pos + "\"command\"".len();
                continue;
            };
            let known = ["command", "working_dir", "timeout_secs"];
            let all_known = obj.keys().all(|k| known.contains(&k.as_str()));
            let has_command_string = obj.get("command").and_then(|c| c.as_str()).is_some();
            if !all_known || !has_command_string {
                search_from = anchor_pos + "\"command\"".len();
                continue;
            }
            tool_calls.push(("bash".to_string(), v));
            strip_ranges.push((obj_start, obj_start + consumed));
            search_from = obj_start + consumed;
        }
    }

    if has_invoke_signal {
        let invoke_calls = extract_invoke_style_tool_calls(text, &strip_ranges);
        for (s, e, name, args) in invoke_calls {
            tool_calls.push((name, args));
            strip_ranges.push((s, e));
        }
        widen_strip_to_known_wrappers(text, &mut strip_ranges);
    }

    if has_bare_name_args {
        for m in super::bare_tool_call_extractor::extract_bare_name_args_calls(
            text,
            &strip_ranges,
            &tool_calls,
        ) {
            if !m.already_in_existing {
                tool_calls.push((m.name, m.args));
            }
            strip_ranges.push((m.strip_start, m.strip_end));
        }
    }

    if strip_ranges.is_empty() {
        return (tool_calls, text.to_string());
    }

    strip_ranges.sort_by_key(|(s, _)| *s);

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (s, e) in strip_ranges {
        if s >= cursor {
            if s > cursor {
                out.push_str(&text[cursor..s]);
            }
            cursor = e;
        } else if e > cursor {
            cursor = e;
        }
    }
    if cursor < text.len() {
        out.push_str(&text[cursor..]);
    }
    (tool_calls, out.trim().to_string())
}

fn extract_claude_style_tool_calls(text: &str) -> Vec<(usize, usize, String, serde_json::Value)> {
    let mut results = Vec::new();
    let mut cursor = 0;

    while cursor < text.len() {
        let mut best: Option<(usize, &'static str)> = None;
        for &tool in KNOWN_TOOL_NAMES {
            let needle_owned = format!("<{}>", tool);
            if let Some(rel) = text[cursor..].find(&needle_owned) {
                let abs = cursor + rel;
                if best.is_none_or(|(b, _)| abs < b) {
                    best = Some((abs, tool));
                }
            }
        }
        let Some((start, tool_name)) = best else {
            break;
        };
        let open_tag_len = tool_name.len() + 2;
        let body_start = start + open_tag_len;

        let close_tag = format!("</{}>", tool_name);
        let Some(close_rel) = text[body_start..].find(&close_tag) else {
            cursor = body_start;
            continue;
        };
        let close_abs = body_start + close_rel;
        let body = &text[body_start..close_abs];

        let params = parse_xml_param_pairs(body);
        if params.is_empty() {
            cursor = close_abs + close_tag.len();
            continue;
        }

        let mut map = serde_json::Map::new();
        for (k, v) in params {
            map.insert(k, serde_json::Value::String(v));
        }
        let end = close_abs + close_tag.len();
        results.push((
            start,
            end,
            tool_name.to_string(),
            serde_json::Value::Object(map),
        ));
        cursor = end;
    }
    results
}

fn parse_xml_param_pairs(body: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut cursor = 0;
    while cursor < body.len() {
        let Some(lt_rel) = body[cursor..].find('<') else {
            break;
        };
        let lt_abs = cursor + lt_rel;
        let after_lt = &body[lt_abs + 1..];
        let name_len = after_lt
            .bytes()
            .take_while(|&b| b.is_ascii_alphanumeric() || b == b'_')
            .count();
        if name_len == 0 || after_lt.as_bytes().get(name_len) != Some(&b'>') {
            cursor = lt_abs + 1;
            continue;
        }
        let name = &after_lt[..name_len];
        let body_start = lt_abs + 1 + name_len + 1;
        let close = format!("</{}>", name);
        let Some(close_rel) = body[body_start..].find(&close) else {
            break;
        };
        let value = body[body_start..body_start + close_rel].trim().to_string();
        pairs.push((name.to_string(), value));
        cursor = body_start + close_rel + close.len();
    }
    pairs
}

fn extract_invoke_style_tool_calls(
    text: &str,
    existing_strip_ranges: &[(usize, usize)],
) -> Vec<(usize, usize, String, serde_json::Value)> {
    let mut results: Vec<(usize, usize, String, serde_json::Value)> = Vec::new();
    let mut cursor: usize = 0;
    while cursor < text.len() {
        let invoke_at = text[cursor..]
            .find("invoke name=")
            .map(|r| (cursor + r, "invoke name=".len()));
        let param_at = text[cursor..]
            .find("<parameter name=")
            .map(|r| (cursor + r, "<parameter name=".len()));
        let (anchor, anchor_len) = match (invoke_at, param_at) {
            (Some(i), Some(p)) => {
                if i.0 <= p.0 {
                    i
                } else {
                    p
                }
            }
            (Some(i), None) => i,
            (None, Some(p)) => p,
            (None, None) => break,
        };
        if existing_strip_ranges
            .iter()
            .any(|(s, e)| *s <= anchor && anchor < *e)
        {
            cursor = anchor + anchor_len;
            continue;
        }
        let after_eq = anchor + anchor_len;
        let bytes = text.as_bytes();
        if after_eq >= bytes.len() {
            break;
        }
        let quote = bytes[after_eq];
        if quote != b'"' && quote != b'\'' {
            cursor = after_eq;
            continue;
        }
        let name_start = after_eq + 1;
        let name_end_rel = match text[name_start..].find(quote as char) {
            Some(r) => r,
            None => {
                cursor = name_start;
                continue;
            }
        };
        let name = text[name_start..name_start + name_end_rel].to_string();
        if !KNOWN_TOOL_NAMES.iter().any(|&n| n == name) {
            cursor = name_start + name_end_rel;
            continue;
        }
        let body_search_start = name_start + name_end_rel;
        let close = text[body_search_start..]
            .find("</invoke>")
            .map(|r| (body_search_start + r, "</invoke>".len()));
        let (body_end, close_len) = close.unwrap_or_else(|| {
            let qwen_close = text[body_search_start..].find("</qwen:tool_call>");
            let func_close = text[body_search_start..].find("</function_calls>");
            let stop_rel = [qwen_close, func_close]
                .into_iter()
                .flatten()
                .min()
                .unwrap_or(text.len() - body_search_start);
            (body_search_start + stop_rel, 0)
        });

        let body = &text[body_search_start..body_end];
        let params = parse_invoke_parameters(body);
        let mut obj = serde_json::Map::new();
        for (k, v) in params {
            obj.insert(k, coerce_xml_param_value(&v));
        }
        let mut block_start = anchor;
        if block_start > 0 && text.as_bytes()[block_start - 1] == b'<' {
            block_start -= 1;
        }
        let block_end = body_end + close_len;
        results.push((block_start, block_end, name, serde_json::Value::Object(obj)));
        cursor = block_end;
    }
    results
}

fn parse_invoke_parameters(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while cursor < body.len() {
        let rel = match body[cursor..].find("<parameter name=") {
            Some(r) => r,
            None => break,
        };
        let abs = cursor + rel;
        let after = abs + "<parameter name=".len();
        let bytes = body.as_bytes();
        if after >= bytes.len() {
            break;
        }
        let quote = bytes[after];
        if quote != b'"' && quote != b'\'' {
            cursor = after;
            continue;
        }
        let name_start = after + 1;
        let name_end_rel = match body[name_start..].find(quote as char) {
            Some(r) => r,
            None => break,
        };
        let key = body[name_start..name_start + name_end_rel].to_string();
        let after_name = name_start + name_end_rel + 1;
        let value_start = match body[after_name..].find('>') {
            Some(r) => after_name + r + 1,
            None => break,
        };
        let close = "</parameter>";
        let close_rel = match body[value_start..].find(close) {
            Some(r) => r,
            None => break,
        };
        let value = body[value_start..value_start + close_rel]
            .trim()
            .to_string();
        if !KNOWN_TOOL_NAMES.iter().any(|&n| n == key) {
            out.push((key, value));
        }
        cursor = value_start + close_rel + close.len();
    }
    out
}

fn coerce_xml_param_value(raw: &str) -> serde_json::Value {
    let trimmed = raw.trim();
    if let Ok(n) = trimmed.parse::<i64>() {
        return serde_json::Value::from(n);
    }
    if let Ok(n) = trimmed.parse::<f64>()
        && n.is_finite()
    {
        return serde_json::json!(n);
    }
    match trimmed.to_ascii_lowercase().as_str() {
        "true" | "yes" => return serde_json::Value::Bool(true),
        "false" | "no" => return serde_json::Value::Bool(false),
        _ => {}
    }
    serde_json::Value::String(trimmed.to_string())
}

fn widen_strip_to_known_wrappers(text: &str, strip_ranges: &mut Vec<(usize, usize)>) {
    const WRAPPERS: &[(&str, &str)] = &[
        ("<qwen:tool_call>", "</qwen:tool_call>"),
        ("<function_calls>", "</function_calls>"),
    ];
    let mut additions: Vec<(usize, usize)> = Vec::new();
    for (open, close) in WRAPPERS {
        let mut search_from = 0;
        while let Some(rel) = text[search_from..].find(open) {
            let open_pos = search_from + rel;
            let after_open = open_pos + open.len();
            let close_rel = match text[after_open..].find(close) {
                Some(r) => r,
                None => break,
            };
            let close_end = after_open + close_rel + close.len();
            let contains_invoke = strip_ranges
                .iter()
                .any(|(s, _)| open_pos <= *s && *s < close_end);
            if contains_invoke {
                additions.push((open_pos, close_end));
            }
            search_from = close_end;
        }
    }
    strip_ranges.extend(additions);
}

/// Fallback parser for Qwen's hallucinated `{"tool_call" {...}}` envelope
/// when the model drops the `:` after keys (seen in logs 2026-04-17).
/// The strict JSON path on line ~1059 fails on this shape, so we
/// recover just `name` + `arguments` by scanning for the bare tokens
/// `"name" "value"` and `"arguments" { ... }`. Returns `None` when the
/// envelope doesn't have the expected structure.
fn parse_malformed_singular_envelope(
    body: &str,
) -> Option<(String, serde_json::Value)> {
    // Find `"name" "<value>"` — value is a quoted string, no colon needed.
    let name_re = find_quoted_pair(body, "name")?;
    // Find `"arguments" { ... }` — value is a balanced JSON-ish object.
    let args_re = find_balanced_after_key(body, "arguments")?;
    let args_val: serde_json::Value = serde_json::from_str(&body[args_re.0..args_re.1])
        .ok()
        .or_else(|| {
            // Args themselves may also be malformed (e.g. `"command" "git status"`
            // with no colon). Try a regex recovery for primitive string pairs.
            let s = &body[args_re.0..args_re.1];
            recover_primitive_string_object(s)
        })?;
    Some((name_re, args_val))
}

/// Scan `body` for a `"<key>" "<value>"` quoted-string pair (no colon
/// required between them). Returns the value when both sides are valid
/// quoted strings and there's only whitespace between them.
fn find_quoted_pair(body: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let key_pos = body.find(&needle)?;
    let after_key = key_pos + needle.len();
    let bytes = body.as_bytes();
    let mut i = after_key;
    while i < bytes.len() && (bytes[i] as char).is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'"' {
        return None;
    }
    let start = i + 1;
    let rest = &body[start..];
    let end_rel = rest.find('"')?;
    Some(rest[..end_rel].to_string())
}

/// Find the balanced `{ ... }` block that follows a `"<key>"` token in
/// `body`. Returns the absolute byte range of the object (including
/// braces). Returns `None` when no object follows the key.
fn find_balanced_after_key(body: &str, key: &str) -> Option<(usize, usize)> {
    let needle = format!("\"{}\"", key);
    let key_pos = body.find(&needle)?;
    let after_key = key_pos + needle.len();
    let bytes = body.as_bytes();
    let mut i = after_key;
    while i < bytes.len() && (bytes[i] as char).is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'{' {
        return None;
    }
    let consumed = extract_balanced_json(&body[i..])?;
    Some((i, i + consumed))
}

/// Recover a JSON object from a string where primitive-valued keys are
/// missing their colons. Handles shapes like:
///   `{"command" "git status"}`  →  `{"command": "git status"}`
///   `{"command" "git status" "timeout_secs" 30}` (with int value)
fn recover_primitive_string_object(s: &str) -> Option<serde_json::Value> {
    let trimmed = s.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return None;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let mut map = serde_json::Map::new();
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip whitespace
        while i < bytes.len() && (bytes[i] as char).is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        if bytes[i] != b'"' {
            return None;
        }
        // Read key
        let key_start = i + 1;
        let key_end_rel = inner[key_start..].find('"')?;
        let key = &inner[key_start..key_start + key_end_rel];
        i = key_start + key_end_rel + 1;
        // Skip whitespace
        while i < bytes.len() && (bytes[i] as char).is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        // Read value
        if bytes[i] == b'"' {
            // String value
            let val_start = i + 1;
            let val_end_rel = inner[val_start..].find('"')?;
            let val = &inner[val_start..val_start + val_end_rel];
            map.insert(
                key.to_string(),
                serde_json::Value::String(val.to_string()),
            );
            i = val_start + val_end_rel + 1;
        } else if bytes[i] == b'{' {
            // Nested object — recurse
            let consumed = extract_balanced_json(&inner[i..])?;
            let nested_str = &inner[i..i + consumed];
            let nested = serde_json::from_str::<serde_json::Value>(nested_str)
                .ok()
                .or_else(|| recover_primitive_string_object(nested_str))?;
            map.insert(key.to_string(), nested);
            i += consumed;
        } else {
            // Try to read a primitive (number, bool, null)
            let start = i;
            while i < bytes.len()
                && !((bytes[i] as char).is_ascii_whitespace() || bytes[i] == b',')
            {
                i += 1;
            }
            let lit = inner[start..i].trim();
            let val = serde_json::from_str::<serde_json::Value>(lit).ok()?;
            map.insert(key.to_string(), val);
        }
    }
    Some(serde_json::Value::Object(map))
}

fn parse_function_params(body: &str) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    let mut i = 0usize;
    while let Some(rel) = body[i..].find("<parameter=") {
        let tag_start = i + rel;
        let after = &body[tag_start..];
        let Some(gt) = after.find('>') else { break };
        let key = body[tag_start + "<parameter=".len()..tag_start + gt].trim();
        if key.is_empty() {
            i = tag_start + gt + 1;
            continue;
        }
        let val_start = tag_start + gt + 1;
        let tail = &body[val_start..];
        let end_at_param = tail.find("</parameter>");
        let end_at_next = tail.find("<parameter=");
        let end = match (end_at_param, end_at_next) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let (val, next_i) = match end {
            Some(rel) => {
                let skip = if end_at_param == Some(rel) {
                    rel + "</parameter>".len()
                } else {
                    rel
                };
                (tail[..rel].trim().to_string(), val_start + skip)
            }
            None => (tail.trim().to_string(), body.len()),
        };
        map.insert(key.to_string(), serde_json::Value::String(val));
        i = next_i;
    }
    serde_json::Value::Object(map)
}
