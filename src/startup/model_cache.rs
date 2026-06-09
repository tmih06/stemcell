//! On-disk cache of provider model lists, warmed at startup.
//!
//! The `/models` dialog pays a live network fetch every time it opens. The
//! [`fetch_models`](super::jobs::fetch_models) startup job warms this cache at
//! boot so the dialog can populate instantly from disk, then refresh in the
//! background.
//!
//! Persisted to `startup_models_cache.json` in the opencrabs base dir,
//! following the `claude_cli_models.json` precedent.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// How long a cached entry is considered fresh enough to skip a live fetch.
pub const FRESH_TTL_SECS: u64 = 24 * 60 * 60;

/// One provider's cached model list plus when it was fetched (epoch secs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEntry {
    pub models: Vec<String>,
    pub fetched_at: u64,
}

/// Provider name → its cached entry.
pub type ModelCache = HashMap<String, CachedEntry>;

/// Path to the cache file. Test builds use a temp-dir override.
#[cfg(not(test))]
fn cache_path() -> PathBuf {
    crate::config::profile::base_opencrabs_dir().join("startup_models_cache.json")
}

#[cfg(test)]
thread_local! {
    static TEST_CACHE_PATH: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn cache_path() -> PathBuf {
    TEST_CACHE_PATH.with(|p| {
        p.borrow()
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join("opencrabs-model-cache-unset.json"))
    })
}

#[cfg(test)]
pub(crate) fn set_test_cache_path(path: PathBuf) {
    TEST_CACHE_PATH.with(|p| *p.borrow_mut() = Some(path));
}

/// Load the cache from disk, or an empty cache if missing/corrupt.
pub fn load() -> ModelCache {
    std::fs::read_to_string(cache_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Read the cached models for one provider, if present and non-empty.
pub fn models_for(provider: &str) -> Option<Vec<String>> {
    load()
        .get(provider)
        .map(|e| e.models.clone())
        .filter(|m| !m.is_empty())
}

/// True when a non-empty entry exists and was fetched within `max_age_secs`.
pub fn is_fresh(provider: &str, max_age_secs: u64) -> bool {
    load().get(provider).is_some_and(|e| {
        !e.models.is_empty() && now_epoch().saturating_sub(e.fetched_at) < max_age_secs
    })
}

/// One-load warm-start lookup: returns the provider's cached models (if present
/// and non-empty) and whether the entry is fresh within `max_age_secs`. Folds
/// what would otherwise be back-to-back [`models_for`] + [`is_fresh`] calls into
/// a single disk read + parse on the `/models` open path.
pub fn warm_start(provider: &str, max_age_secs: u64) -> (Option<Vec<String>>, bool) {
    match load().get(provider) {
        Some(e) if !e.models.is_empty() => {
            let fresh = now_epoch().saturating_sub(e.fetched_at) < max_age_secs;
            (Some(e.models.clone()), fresh)
        }
        _ => (None, false),
    }
}

/// Insert/replace one provider's models and persist. Silently ignores IO
/// errors — a failed write just means `/models` falls back to a live fetch.
pub fn store(provider: &str, models: Vec<String>) {
    let mut cache = load();
    cache.insert(
        provider.to_string(),
        CachedEntry {
            models,
            fetched_at: now_epoch(),
        },
    );
    let path = cache_path();
    if let Ok(json) = serde_json::to_string_pretty(&cache) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, json);
    }
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_store_and_read() {
        let dir = std::env::temp_dir().join(format!("oc-model-cache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("startup_models_cache.json");
        let _ = std::fs::remove_file(&path);
        set_test_cache_path(path.clone());

        assert!(models_for("openai").is_none());

        store("openai", vec!["gpt-5".into(), "gpt-4".into()]);
        let got = models_for("openai").unwrap();
        assert_eq!(got, vec!["gpt-5".to_string(), "gpt-4".to_string()]);

        // A second provider coexists.
        store("anthropic", vec!["opus".into()]);
        assert_eq!(models_for("anthropic").unwrap(), vec!["opus".to_string()]);
        assert_eq!(models_for("openai").unwrap().len(), 2);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_models_treated_as_absent() {
        let dir = std::env::temp_dir().join(format!("oc-model-cache-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("startup_models_cache.json");
        let _ = std::fs::remove_file(&path);
        set_test_cache_path(path.clone());

        store("ollama", vec![]);
        assert!(models_for("ollama").is_none());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn freshness_respects_ttl() {
        let dir = std::env::temp_dir().join(format!("oc-model-cache-fresh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("startup_models_cache.json");
        let _ = std::fs::remove_file(&path);
        set_test_cache_path(path.clone());

        // Missing → not fresh.
        assert!(!is_fresh("openai", FRESH_TTL_SECS));

        // Just-stored → fresh within a generous TTL.
        store("openai", vec!["gpt-5".into()]);
        assert!(is_fresh("openai", FRESH_TTL_SECS));

        // Same entry is stale under a zero TTL.
        assert!(!is_fresh("openai", 0));

        // An entry with an ancient fetched_at is not fresh.
        let mut cache = load();
        cache.insert(
            "anthropic".to_string(),
            CachedEntry {
                models: vec!["opus".into()],
                fetched_at: 1,
            },
        );
        std::fs::write(&path, serde_json::to_string_pretty(&cache).unwrap()).unwrap();
        assert!(!is_fresh("anthropic", FRESH_TTL_SECS));

        let _ = std::fs::remove_file(&path);
    }
}
