use crossterm::event::{KeyCode, KeyEvent};

use super::types::*;
use super::wizard::OnboardingWizard;

impl OnboardingWizard {
    pub(super) fn handle_voice_setup_key(&mut self, event: KeyEvent) -> WizardAction {
        super::voice::handle_key(self, event)
    }

    pub(super) fn handle_image_setup_key(&mut self, event: KeyEvent) -> WizardAction {
        let either_enabled = self.image_vision_enabled || self.image_generation_enabled;

        match self.image_field {
            ImageField::VisionToggle => match event.code {
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.image_vision_enabled = !self.image_vision_enabled;
                }
                KeyCode::Tab | KeyCode::Enter => {
                    self.image_field = ImageField::GenerationToggle;
                }
                _ => {}
            },
            ImageField::GenerationToggle => match event.code {
                KeyCode::Char(' ') | KeyCode::Up | KeyCode::Down => {
                    self.image_generation_enabled = !self.image_generation_enabled;
                }
                KeyCode::BackTab => {
                    self.image_field = ImageField::VisionToggle;
                }
                KeyCode::Tab | KeyCode::Enter => {
                    if self.image_generation_enabled {
                        self.image_field = ImageField::GenerationModel;
                    } else if either_enabled {
                        self.image_field = ImageField::ApiKey;
                    } else {
                        self.next_step();
                    }
                }
                _ => {}
            },
            ImageField::GenerationModel => match event.code {
                KeyCode::Char(c) => {
                    self.image_generation_model_input.push(c);
                }
                KeyCode::Backspace => {
                    self.image_generation_model_input.pop();
                }
                KeyCode::BackTab => {
                    self.image_field = ImageField::GenerationToggle;
                }
                KeyCode::Tab | KeyCode::Enter => {
                    self.image_field = ImageField::ApiKey;
                }
                _ => {}
            },
            ImageField::ApiKey => match event.code {
                KeyCode::Char(c) => {
                    if self.has_existing_image_key() {
                        self.image_api_key_input.clear();
                    }
                    self.image_api_key_input.push(c);
                }
                KeyCode::Backspace => {
                    if self.has_existing_image_key() {
                        self.image_api_key_input.clear();
                    } else {
                        self.image_api_key_input.pop();
                    }
                }
                KeyCode::BackTab => {
                    // Skip back over GenerationModel only when generation
                    // is enabled — otherwise it never got navigated to.
                    self.image_field = if self.image_generation_enabled {
                        ImageField::GenerationModel
                    } else {
                        ImageField::GenerationToggle
                    };
                }
                KeyCode::Enter => {
                    self.next_step();
                }
                _ => {}
            },
        }
        WizardAction::None
    }

    pub(super) fn handle_daemon_key(&mut self, event: KeyEvent) -> WizardAction {
        match event.code {
            KeyCode::Up | KeyCode::Down | KeyCode::Char(' ') => {
                self.install_daemon = !self.install_daemon;
            }
            KeyCode::Enter => {
                self.next_step();
            }
            _ => {}
        }
        WizardAction::None
    }

    pub(super) fn handle_health_check_key(&mut self, event: KeyEvent) -> WizardAction {
        match event.code {
            KeyCode::Enter if self.quick_jump && self.health_complete => {
                // Re-run checks on Enter after complete
                self.start_health_check();
            }
            KeyCode::Enter if self.health_complete => {
                self.next_step();
                return WizardAction::None;
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                self.start_health_check();
            }
            _ => {}
        }
        WizardAction::None
    }
}

/// First-time detection: no config file AND no API keys in environment.
/// Once config.toml is written (by onboarding or manually), this returns false forever.
/// If any API key env var is set, the user has already configured auth — skip onboarding.
/// To re-run the wizard, use `stemcell onboard`, `--onboard` flag, or `/onboard`.
pub fn is_first_time() -> bool {
    tracing::debug!("[is_first_time] checking if first time setup needed...");

    // Check if config exists
    let config_path = crate::config::stemcell_home().join("config.toml");
    if !config_path.exists() {
        tracing::debug!("[is_first_time] no config found, need onboarding");
        return true;
    }

    // Config exists - check if any provider is actually enabled
    let config = match crate::config::Config::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(
                "[is_first_time] failed to load config: {}, need onboarding",
                e
            );
            return true;
        }
    };

    let has_enabled_provider = config
        .providers
        .anthropic
        .as_ref()
        .is_some_and(|p| p.enabled)
        || config.providers.openai.as_ref().is_some_and(|p| p.enabled)
        || config.providers.github.as_ref().is_some_and(|p| p.enabled)
        || config.providers.gemini.as_ref().is_some_and(|p| p.enabled)
        || config
            .providers
            .openrouter
            .as_ref()
            .is_some_and(|p| p.enabled)
        || config.providers.minimax.as_ref().is_some_and(|p| p.enabled)
        || config.providers.zhipu.as_ref().is_some_and(|p| p.enabled)
        || config
            .providers
            .claude_cli
            .as_ref()
            .is_some_and(|p| p.enabled)
        || config
            .providers
            .opencode_cli
            .as_ref()
            .is_some_and(|p| p.enabled)
        || config
            .providers
            .codex_cli
            .as_ref()
            .is_some_and(|p| p.enabled)
        || config.providers.codex.as_ref().is_some_and(|p| p.enabled)
        || config.providers.qwen.as_ref().is_some_and(|p| p.enabled)
        || config.providers.ollama.as_ref().is_some_and(|p| p.enabled)
        || config
            .providers
            .opencode
            .as_ref()
            .is_some_and(|p| p.enabled)
        || config.providers.active_custom().is_some();

    tracing::debug!(
        "[is_first_time] has_enabled_provider={}, result={}",
        has_enabled_provider,
        !has_enabled_provider
    );
    !has_enabled_provider
}

/// Fetch models from provider API. No API key needed for most providers.
/// If api_key is provided, includes it (some endpoints filter by access level).
/// For custom providers, pass base_url to fetch from the endpoint.
/// Returns empty vec on failure (callers fall back to static list).
pub async fn fetch_provider_models(
    provider_index: usize,
    api_key: Option<&str>,
    zhipu_endpoint_type: Option<&str>,
    base_url: Option<&str>,
) -> Vec<String> {
    use crate::tui::onboarding::PROVIDERS;
    let provider_id = PROVIDERS.get(provider_index).map(|p| p.id).unwrap_or("");
    tracing::info!(
        "[fetch_provider_models] provider_index={}, provider_id={}, has_api_key={}",
        provider_index,
        provider_id,
        api_key.is_some(),
    );
    #[derive(serde::Deserialize)]
    struct ModelEntry {
        id: String,
        #[serde(default)]
        created: i64,
    }
    #[derive(serde::Deserialize)]
    struct ModelsResponse {
        data: Vec<ModelEntry>,
    }

    // Claude CLI — models are fixed (sonnet/opus/haiku), no API needed
    if provider_id == "claude-cli" {
        return vec![
            "sonnet".to_string(),
            "opus".to_string(),
            "haiku".to_string(),
        ];
    }

    // OpenCode CLI — fetch models via `opencode models` command
    if provider_id == "opencode-cli" {
        return fetch_opencode_models().await;
    }

    // Codex CLI & Codex OAuth — model list is curated; no /v1/models endpoint.
    if provider_id == "codex-cli" || provider_id == "codex" {
        let config_key = if provider_id == "codex" {
            "codex"
        } else {
            "codex-cli"
        };
        let models = crate::tui::provider_selector::load_default_models(config_key);
        if !models.is_empty() {
            return models;
        }
        return vec![
            "gpt-5.5".to_string(),
            "gpt-5.4".to_string(),
            "gpt-5.4-mini".to_string(),
            "gpt-5.3-codex".to_string(),
            "gpt-5.3-codex-spark".to_string(),
            "gpt-5.2".to_string(),
        ];
    }

    // Qwen (DashScope): no /v1/models endpoint on the OpenAI-compat path,
    // so we read the curated list from config.toml.example. Users can
    // override via `models = [...]` in their own config.toml.
    if provider_id == "qwen" {
        let models = crate::tui::provider_selector::load_default_models("qwen");
        if !models.is_empty() {
            return models;
        }
        return vec![
            "qwen3.6-plus".to_string(),
            "qwen3-max".to_string(),
            "qwen3-coder-plus".to_string(),
            "qwen3.5-plus".to_string(),
            "qwen-max".to_string(),
            "qwen-plus".to_string(),
            "qwen-flash".to_string(),
        ];
    }

    // MiniMax — supports OpenAI-compatible /v1/models on both
    // international (api.minimax.io) and China (api.minimaxi.com).
    // Fall back to the curated baseline if the fetch fails (no
    // network, older account tier, etc.).
    if provider_id == "minimax" {
        return fetch_minimax_models(api_key).await;
    }

    let client = reqwest::Client::new();

    let result = match provider_id {
        "anthropic" => {
            // Anthropic — /v1/models is public
            let mut req = client
                .get("https://api.anthropic.com/v1/models")
                .header("anthropic-version", "2023-06-01");

            // Include key if available (may show more models)
            if let Some(key) = api_key {
                if key.starts_with("sk-ant-oat") {
                    req = req
                        .header("Authorization", format!("Bearer {}", key))
                        .header("anthropic-beta", "oauth-2025-04-20");
                } else if !key.is_empty() {
                    req = req.header("x-api-key", key);
                }
            }

            req.send().await
        }
        "openai" => {
            // OpenAI — /v1/models
            let mut req = client.get("https://api.openai.com/v1/models");
            if let Some(key) = api_key
                && !key.is_empty()
            {
                req = req.header("Authorization", format!("Bearer {}", key));
            }
            req.send().await
        }
        "github" => {
            // GitHub Copilot — fetch from Copilot API using OAuth token
            if let Some(key) = api_key
                && !key.is_empty()
            {
                match crate::brain::provider::copilot::fetch_copilot_models(key).await {
                    Ok(models) if !models.is_empty() => return models,
                    Ok(_) => tracing::debug!("Copilot models endpoint returned empty list"),
                    Err(e) => tracing::debug!("Copilot models fetch failed: {}", e),
                }
            }
            // Fall back to config or defaults
            if let Ok(config) = crate::config::Config::load()
                && let Some(p) = &config.providers.github
            {
                if !p.models.is_empty() {
                    return p.models.clone();
                }
                if let Some(model) = &p.default_model {
                    return vec![model.clone()];
                }
            }
            return crate::tui::provider_selector::load_default_models("github");
        }
        "gemini" => {
            // Google Gemini — list models via generativelanguage API
            let key = match api_key {
                Some(k) if !k.is_empty() => k,
                _ => {
                    tracing::warn!(
                        "[fetch_provider_models] Gemini: no API key provided, returning empty"
                    );
                    return Vec::new();
                }
            };
            tracing::info!("[fetch_provider_models] Gemini: fetching models (key present)");
            let url = "https://generativelanguage.googleapis.com/v1beta/models";
            // Gemini uses a different response shape: { models: [{ name: "models/gemini-..." }] }
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct GeminiModel {
                name: String,
                #[serde(default)]
                supported_generation_methods: Vec<String>,
            }
            #[derive(serde::Deserialize)]
            struct GeminiModelsResponse {
                models: Vec<GeminiModel>,
            }
            match client.get(url).header("x-goog-api-key", key).send().await {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<GeminiModelsResponse>().await {
                        Ok(body) => {
                            let mut models: Vec<String> = body
                                .models
                                .into_iter()
                                .filter(|m| {
                                    m.supported_generation_methods
                                        .iter()
                                        .any(|g| g == "generateContent")
                                })
                                .map(|m| {
                                    m.name
                                        .strip_prefix("models/")
                                        .unwrap_or(&m.name)
                                        .to_string()
                                })
                                .collect();
                            models.sort();
                            models.reverse(); // Newest model versions first
                            tracing::info!(
                                "[fetch_provider_models] Gemini: fetched {} models",
                                models.len()
                            );
                            return models;
                        }
                        Err(e) => {
                            tracing::warn!("Gemini models parse error: {}", e);
                            return Vec::new();
                        }
                    }
                }
                Ok(resp) => {
                    tracing::warn!("Gemini models API returned {}", resp.status());
                    return Vec::new();
                }
                Err(e) => {
                    tracing::warn!("Gemini models fetch failed: {}", e);
                    return Vec::new();
                }
            }
        }
        "openrouter" => {
            // OpenRouter — /api/v1/models
            let mut req = client.get("https://openrouter.ai/api/v1/models");
            if let Some(key) = api_key
                && !key.is_empty()
            {
                req = req.header("Authorization", format!("Bearer {}", key));
            }
            req.send().await
        }
        "opencode" => {
            // OpenCode API — /zen/go/v1/models (Go and Zen plans)
            let mut req = client.get("https://opencode.ai/zen/go/v1/models");
            if let Some(key) = api_key
                && !key.is_empty()
            {
                req = req.header("Authorization", format!("Bearer {}", key));
            }
            req.send().await
        }
        "opencode_zen_free" => {
            // OpenCode Zen Free API
            // Filter models dynamically based on cost from models.dev, matching OpenCode's source
            let req = client.get("https://models.dev/api.json");
            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if let Some(models) =
                            json.pointer("/opencode/models").and_then(|v| v.as_object())
                        {
                            let mut free_models: Vec<String> = models
                                .iter()
                                .filter_map(|(id, model)| {
                                    let is_free = model
                                        .get("cost")
                                        .and_then(|c| c.get("input"))
                                        .and_then(|i| i.as_f64())
                                        .map_or(true, |input| input == 0.0);
                                    if is_free {
                                        Some(id.clone())
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            free_models.sort();
                            tracing::info!(
                                "[fetch_provider_models] OpenCode Zen Free: fetched {} free models",
                                free_models.len()
                            );
                            return free_models;
                        }
                    }
                }
                Ok(resp) => tracing::warn!("models.dev API returned {}", resp.status()),
                Err(e) => tracing::warn!("models.dev fetch failed: {}", e),
            }
            return Vec::new();
        }
        "zhipu" => {
            // z.ai GLM — /api/paas/v4/models or /api/coding/paas/v4/models
            // Use passed endpoint_type (from wizard state), fall back to config, then default "api"
            let endpoint_type = zhipu_endpoint_type
                .map(|s| s.to_string())
                .or_else(|| {
                    crate::config::Config::load()
                        .ok()
                        .and_then(|c| c.providers.zhipu.clone())
                        .and_then(|p| p.endpoint_type)
                })
                .unwrap_or_else(|| "api".to_string());

            let base = match endpoint_type.as_str() {
                "coding" => "https://api.z.ai/api/coding/paas/v4/models",
                _ => "https://api.z.ai/api/paas/v4/models",
            };

            let mut req = client.get(base);
            if let Some(key) = api_key
                && !key.is_empty()
            {
                req = req.header("Authorization", format!("Bearer {}", key));
            }
            req.send().await
        }
        "ollama" => {
            // Ollama — fetch from /api/tags (local or cloud)
            let base = if let Some(url) = base_url
                && !url.is_empty()
            {
                url.to_string()
            } else {
                "http://localhost:11434".to_string()
            };
            let base = base.trim_end_matches('/');
            #[derive(serde::Deserialize)]
            struct OllamaModel {
                name: String,
            }
            #[derive(serde::Deserialize)]
            struct OllamaModelsResponse {
                models: Vec<OllamaModel>,
            }
            let mut req = client.get(format!("{}/api/tags", base));
            if let Some(key) = api_key
                && !key.is_empty()
            {
                req = req.header("Authorization", format!("Bearer {}", key));
            }
            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<OllamaModelsResponse>().await {
                        Ok(body) => {
                            let mut models: Vec<String> =
                                body.models.into_iter().map(|m| m.name).collect();
                            models.sort();
                            models.reverse();
                            tracing::info!(
                                "[fetch_provider_models] Ollama: fetched {} models",
                                models.len()
                            );
                            return models;
                        }
                        Err(e) => {
                            tracing::warn!("Ollama models parse error: {}", e);
                            return Vec::new();
                        }
                    }
                }
                Ok(resp) => {
                    tracing::warn!("Ollama models API returned {}", resp.status());
                    return Vec::new();
                }
                Err(e) => {
                    tracing::warn!("Ollama models fetch failed: {}", e);
                    return Vec::new();
                }
            }
        }
        _ => {
            // Custom provider: try fetching from base_url if provided
            if let Some(url) = base_url
                && !url.is_empty()
            {
                return crate::brain::provider::model_fetch::fetch_models_from_endpoint(
                    url, api_key,
                )
                .await;
            }
            return Vec::new();
        }
    };

    match result {
        Ok(resp) if resp.status().is_success() => match resp.json::<ModelsResponse>().await {
            Ok(body) => {
                let mut entries = body.data;
                // Sort newest first (by created timestamp descending)
                entries.sort_by_key(|e| std::cmp::Reverse(e.created));
                // The chat TUI is the main interface, so drop non-chat models
                // (image / audio / embedding / moderation / …) that providers'
                // /v1/models endpoints mix in.  OpenAI's endpoint is untyped
                // and the worst offender (dall-e, whisper, tts, embeddings),
                // but the id-based filter is safe for every provider here.
                entries
                    .into_iter()
                    .map(|m| m.id)
                    .filter(|id| crate::startup::model_cache::is_chat_capable_model_id(id))
                    .collect()
            }
            Err(_) => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Binary's known MiniMax models. Newest first so the picker
/// highlights the current model. Update this when MiniMax ships new
/// releases — the additive merge will reach users on older configs
/// without them needing to touch `models = [...]` in config.toml.
fn minimax_baseline_models() -> Vec<String> {
    vec![
        "MiniMax-M3".to_string(),
        "MiniMax-M2.7".to_string(),
        "MiniMax-M2.5".to_string(),
        "MiniMax-M2.1".to_string(),
    ]
}

/// User's saved MiniMax models from config.toml, plus the
/// default_model fallback when no list was saved. Empty when no
/// MiniMax provider is configured.
fn user_minimax_models() -> Vec<String> {
    let Ok(config) = crate::config::Config::load() else {
        return Vec::new();
    };
    let Some(p) = &config.providers.minimax else {
        return Vec::new();
    };
    if !p.models.is_empty() {
        return p.models.clone();
    }
    if let Some(model) = &p.default_model {
        return vec![model.clone()];
    }
    Vec::new()
}

/// Fetch MiniMax models live from the OpenAI-compatible /v1/models endpoint.
/// Both international (api.minimax.io) and China (api.minimaxi.com) regions
/// expose this endpoint with the standard OpenAI response shape.  Falls back
/// to the curated baseline on any network/auth failure so the picker is never
/// empty.
async fn fetch_minimax_models(api_key: Option<&str>) -> Vec<String> {
    let api_key = match api_key {
        Some(k) if !k.is_empty() => k,
        _ => return merge_minimax_baseline(minimax_baseline_models(), user_minimax_models()),
    };

    let client = reqwest::Client::new();
    for endpoint in &[
        "https://api.minimax.io/v1/models",
        "https://api.minimaxi.com/v1/models",
    ] {
        let req = client
            .get(*endpoint)
            .header("Authorization", format!("Bearer {}", api_key));
        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                #[derive(serde::Deserialize)]
                struct MiniMaxEntry {
                    id: String,
                }
                #[derive(serde::Deserialize)]
                struct MiniMaxResponse {
                    data: Vec<MiniMaxEntry>,
                }
                match resp.json::<MiniMaxResponse>().await {
                    Ok(body) => {
                        let mut models: Vec<String> = body.data.into_iter().map(|m| m.id).collect();
                        models.sort();
                        models.reverse();
                        tracing::info!(
                            "[fetch_provider_models] MiniMax: fetched {} models from {}",
                            models.len(),
                            endpoint
                        );
                        return models;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "[fetch_provider_models] MiniMax: parse error from {}: {}",
                            endpoint,
                            e
                        );
                    }
                }
            }
            Ok(resp) => {
                tracing::debug!(
                    "[fetch_provider_models] MiniMax: {} returned {}",
                    endpoint,
                    resp.status()
                );
            }
            Err(e) => {
                tracing::debug!(
                    "[fetch_provider_models] MiniMax: {} unreachable: {}",
                    endpoint,
                    e
                );
            }
        }
    }
    tracing::info!(
        "[fetch_provider_models] MiniMax: live fetch failed, falling back to curated list"
    );
    merge_minimax_baseline(minimax_baseline_models(), user_minimax_models())
}

/// Merge MiniMax baseline + user models. Baseline order preserved
/// at the front (so a fresh release like MiniMax-M3 lands at the
/// top of the picker on every binary upgrade); user-only entries
/// appended at the end (so private variants / Text-01 / etc. stay
/// available). Case-insensitive dedup so `MiniMax-M3` and
/// `minimax-m3` don't both appear.
pub(crate) fn merge_minimax_baseline(baseline: Vec<String>, user: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(baseline.len() + user.len());
    let mut seen = std::collections::HashSet::<String>::new();
    for m in baseline {
        let key = m.to_lowercase();
        if seen.insert(key) {
            out.push(m);
        }
    }
    for m in user {
        let key = m.to_lowercase();
        if seen.insert(key) {
            out.push(m);
        }
    }
    out
}

/// Fetch available models from the opencode CLI binary.
async fn fetch_opencode_models() -> Vec<String> {
    // Resolve binary path
    let home = dirs::home_dir().unwrap_or_default();
    let candidates = [
        std::env::var("OPENCODE_PATH").unwrap_or_default(),
        home.join(".opencode/bin/opencode")
            .to_string_lossy()
            .to_string(),
        "/opt/homebrew/bin/opencode".to_string(),
        "/usr/local/bin/opencode".to_string(),
    ];

    let binary = candidates
        .iter()
        .find(|p| !p.is_empty() && std::path::Path::new(p).exists());

    let Some(binary) = binary else {
        // Try `which` as fallback
        if let Ok(output) = tokio::process::Command::new("which")
            .arg("opencode")
            .output()
            .await
            && output.status.success()
        {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return run_opencode_models(&path).await;
            }
        }
        return Vec::new();
    };

    run_opencode_models(binary).await
}

async fn run_opencode_models(binary: &str) -> Vec<String> {
    let output = match tokio::process::Command::new(binary)
        .arg("models")
        .output()
        .await
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut models: Vec<String> = stdout
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('{'))
        .map(|l| l.to_string())
        .collect();
    models.sort();
    models
}
