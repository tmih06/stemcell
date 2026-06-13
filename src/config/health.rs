//! Provider health tracking — records success/failure per provider.
//!
//! Persisted to `~/.stemcell/provider_health.json`. Used for auto-fallback:
//! when the current provider fails, the system can suggest or switch to the
//! last provider that successfully returned a response.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// Per-provider health record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    /// Last successful response timestamp (epoch seconds).
    pub last_success: Option<u64>,
    /// Last failure timestamp (epoch seconds).
    pub last_failure: Option<u64>,
    /// Last error message (truncated to 200 chars).
    pub last_error: Option<String>,
    /// Consecutive failure count (resets on success).
    pub consecutive_failures: u32,
}

/// Health state for all providers.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HealthState {
    pub providers: HashMap<String, ProviderHealth>,
}

/// Global in-memory health state (flushed to disk periodically).
static HEALTH: Mutex<Option<HealthState>> = Mutex::new(None);

fn health_path() -> PathBuf {
    super::stemcell_home().join("provider_health.json")
}

/// Load health state from disk (or initialize empty).
fn ensure_loaded() -> HealthState {
    let mut guard = HEALTH.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref state) = *guard {
        return state.clone();
    }
    let state: HealthState = std::fs::read_to_string(health_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    *guard = Some(state.clone());
    state
}

/// Persist health state to disk. Silently ignores errors.
fn flush(state: &HealthState) {
    let mut guard = HEALTH.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(state.clone());
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(health_path(), json);
    }
}

/// Record a successful provider response.
pub fn record_success(provider_name: &str) {
    let mut state = ensure_loaded();
    let entry = state
        .providers
        .entry(provider_name.to_string())
        .or_insert(ProviderHealth {
            last_success: None,
            last_failure: None,
            last_error: None,
            consecutive_failures: 0,
        });
    entry.last_success = Some(now_epoch());
    entry.consecutive_failures = 0;
    flush(&state);
}

/// Record a provider failure.
pub fn record_failure(provider_name: &str, error: &str) {
    let mut state = ensure_loaded();
    let entry = state
        .providers
        .entry(provider_name.to_string())
        .or_insert(ProviderHealth {
            last_success: None,
            last_failure: None,
            last_error: None,
            consecutive_failures: 0,
        });
    entry.last_failure = Some(now_epoch());
    entry.last_error = Some(error.chars().take(200).collect());
    entry.consecutive_failures += 1;
    flush(&state);
}

/// Get the name of the last provider that succeeded (most recent `last_success`).
/// Returns None if no provider has ever succeeded.
pub fn last_working_provider() -> Option<String> {
    let state = ensure_loaded();
    state
        .providers
        .iter()
        .filter_map(|(name, health)| health.last_success.map(|ts| (name.clone(), ts)))
        .max_by_key(|(_, ts)| *ts)
        .map(|(name, _)| name)
}

/// Get health info for a specific provider.
pub fn get_health(provider_name: &str) -> Option<ProviderHealth> {
    let state = ensure_loaded();
    state.providers.get(provider_name).cloned()
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Run the doctor health check and return the result as plain text.
///
/// Inspects keys.toml, configured providers/channels, voice + approval
/// settings, persisted provider health, and config-recovery snapshot.
/// Used by the TUI `/debug` dump and channel `/doctor` — neither goes
/// through the LLM, so this lives in config (always compiled) rather than
/// on an agent tool.
pub fn doctor_text() -> String {
    let config = match super::Config::load() {
        Ok(c) => c,
        Err(e) => return format!("Failed to load config: {}", e),
    };

    let mut lines = vec!["Health Check".to_string(), String::new()];

    // Check keys.toml validity
    let keys_path = super::keys_path();
    if keys_path.exists() {
        match std::fs::read_to_string(&keys_path) {
            Ok(content) => match toml::from_str::<toml::Value>(&content) {
                Ok(_) => lines.push("keys.toml — OK".to_string()),
                Err(e) => lines.push(format!("keys.toml — PARSE ERROR: {e}")),
            },
            Err(e) => lines.push(format!("keys.toml — READ ERROR: {e}")),
        }
    } else {
        lines.push("keys.toml — NOT FOUND".to_string());
    }
    lines.push(String::new());

    // Check providers — iterate the canonical registry so every built-in
    // provider is covered (a hardcoded list silently hid providers for
    // months; see provider_registry docs).
    lines.push("Providers:".to_string());
    for (id, _display, requires_api_key, provider_opt) in config.providers.provider_registry() {
        if let Some(provider) = provider_opt
            && provider.enabled
        {
            let has_key = provider.api_key.as_ref().is_some_and(|k| !k.is_empty());
            let model = provider.default_model.as_deref().unwrap_or("(not set)");
            let status = if !requires_api_key || has_key {
                "OK"
            } else {
                "MISSING API KEY"
            };
            lines.push(format!("  {} — {} (model: {})", id, status, model));
        }
    }

    if let Some(ref custom) = config.providers.custom {
        for (name, provider) in custom {
            if provider.enabled {
                let has_key = provider.api_key.as_ref().is_some_and(|k| !k.is_empty());
                let model = provider.default_model.as_deref().unwrap_or("(not set)");
                let status = if has_key { "OK" } else { "MISSING API KEY" };
                lines.push(format!("  custom/{} — {} (model: {})", name, status, model));
            }
        }
    }

    // Check channels
    lines.push(String::new());
    lines.push("Channels:".to_string());
    let ch = &config.channels;
    if ch.telegram.enabled {
        lines.push("  telegram — enabled".to_string());
    }
    if ch.discord.enabled {
        lines.push("  discord — enabled".to_string());
    }
    if ch.slack.enabled {
        lines.push("  slack — enabled".to_string());
    }
    if ch.whatsapp.enabled {
        lines.push("  whatsapp — enabled".to_string());
    }
    if ch.trello.enabled {
        lines.push("  trello — enabled".to_string());
    }

    // Voice config
    lines.push(String::new());
    let voice = config.voice_config();
    lines.push(format!(
        "Voice: STT={}, TTS={}",
        voice.stt_enabled, voice.tts_enabled
    ));

    // Approval policy
    lines.push(format!("Approval: {}", config.agent.approval_policy));

    // Provider health
    lines.push(String::new());
    lines.push("Provider Health:".to_string());
    let health_state: HealthState =
        std::fs::read_to_string(super::stemcell_home().join("provider_health.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
    if health_state.providers.is_empty() {
        lines.push("  (no data yet)".to_string());
    } else {
        for (name, h) in &health_state.providers {
            let status = if h.consecutive_failures > 0 {
                format!("FAILING ({}x)", h.consecutive_failures)
            } else {
                "OK".to_string()
            };
            lines.push(format!("  {} — {}", name, status));
        }
    }

    // Last known good config
    let has_good = super::stemcell_home()
        .join("config.last_good.toml")
        .exists();
    lines.push(format!(
        "Config recovery: {}",
        if has_good {
            "snapshot available"
        } else {
            "no snapshot"
        }
    ));

    lines.join("\n")
}
