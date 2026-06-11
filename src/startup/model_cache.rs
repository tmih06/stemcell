//! On-disk cache of provider model lists, warmed at startup.
//!
//! The `/models` dialog pays a live network fetch every time it opens. The
//! [`fetch_models`](super::jobs::fetch_models) startup job warms this cache at
//! boot so the dialog can populate instantly from disk, then refresh in the
//! background.
//!
//! Persisted to `startup_models_cache.json` in the stemcell base dir,
//! following the `claude_cli_models.json` precedent.
//!
//! Each entry holds a list of [`CachedModel`] records. The startup job seeds
//! them from [models.dev](https://models.dev) with rich metadata (display
//! name, `tool_call` / `reasoning` flags, modalities, cost, context limits);
//! per-provider live API fetches yield ids only and are merged in via
//! [`store`], which preserves any models.dev metadata already known for a id.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// How long a cached entry is considered fresh enough to skip a live fetch.
pub const FRESH_TTL_SECS: u64 = 24 * 60 * 60;

/// The chat TUI is the main interface, so the model picker should only list
/// models capable of conversation + tool use.  Provider APIs and catalogs mix
/// in embedding, image, audio, rerank, moderation, and video models that are
/// useless in a chat picker.  Most of those carry a recognizable token in
/// their id, so we drop them by id pattern — provider-agnostic, applied to
/// every auto-discovered list (cache + live fetch).
///
/// This only filters auto-discovered lists; models a user explicitly puts in
/// `config.toml` are never run through it.
pub fn is_chat_capable_model_id(id: &str) -> bool {
    let id = id.to_ascii_lowercase();

    // Exact non-chat families (prefix match).
    const NON_CHAT_PREFIXES: &[&str] = &[
        "dall-e",
        "whisper",
        "tts",
        "text-embedding",
        "text-moderation",
        "omni-moderation",
        "sora",
        "gpt-image",
        "babbage",
        "davinci",
        "computer-use",
        "imagen",
        "veo-",
        "stable-diffusion",
        "stable-image",
        "textembedding",
        "multimodalembedding",
    ];
    if NON_CHAT_PREFIXES.iter().any(|p| id.starts_with(p)) {
        return false;
    }

    // Non-chat capability tokens that appear anywhere in the id.  These cover
    // cross-provider naming (bedrock `cohere.embed-*`, `amazon.titan-embed-*`,
    // `*.rerank-*`, `stability.*` / `nova-canvas` image models, vertex/gemini
    // embedding + image ids, openai size-prefixed `…/gpt-image-*` variants,
    // `-audio` / `-realtime` / `-transcribe` / `-search`, etc.).  Note `image`
    // catches image-generation models — chat vision models use `vision`, not
    // `image`, in their ids, so multimodal chat models are not affected.
    const NON_CHAT_SUBSTRINGS: &[&str] = &[
        "embed",
        "embedding",
        "-tts",
        "-audio",
        "-transcribe",
        "-realtime",
        "-search",
        "rerank",
        "moderation",
        "image",
        "diffusion",
        "upscale",
        "canvas",
        "outpaint",
        "inpaint",
        "stability.",
        "imagegeneration",
    ];
    if NON_CHAT_SUBSTRINGS.iter().any(|s| id.contains(s)) {
        return false;
    }

    true
}

/// Token cost (USD per million tokens) for one model, mirrored from
/// models.dev's `cost` object. All fields optional — providers populate
/// whichever they publish.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ModelCost {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,
}

impl ModelCost {
    fn is_empty(&self) -> bool {
        self == &ModelCost::default()
    }
}

/// Token limits for one model, mirrored from models.dev's `limit` object.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ModelLimit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<u64>,
}

impl ModelLimit {
    fn is_empty(&self) -> bool {
        self == &ModelLimit::default()
    }
}

/// One model's id plus the metadata we mirror from models.dev. Live provider
/// API fetches only know the `id`, so every other field is optional and is
/// filled in (and preserved across merges) when models.dev covers the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedModel {
    /// Native model id (e.g. `claude-opus-4-5`, `gpt-5`).
    pub id: String,
    /// Human-readable display name, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether the model supports tool / function calling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<bool>,
    /// Whether the model supports reasoning / thinking mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    /// Accepted input modalities (e.g. `["text", "image"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_modalities: Vec<String>,
    /// Per-million-token pricing, when published.
    #[serde(default, skip_serializing_if = "ModelCost::is_empty")]
    pub cost: ModelCost,
    /// Context / output token limits, when published.
    #[serde(default, skip_serializing_if = "ModelLimit::is_empty")]
    pub limit: ModelLimit,
}

impl CachedModel {
    /// A bare id with no metadata — used for ids that arrive from a live
    /// provider API fetch with no models.dev coverage.
    pub fn id_only(id: impl Into<String>) -> Self {
        CachedModel {
            id: id.into(),
            name: None,
            tool_call: None,
            reasoning: None,
            input_modalities: Vec::new(),
            cost: ModelCost::default(),
            limit: ModelLimit::default(),
        }
    }
}

/// One provider's cached model list plus when it was fetched (epoch secs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEntry {
    pub models: Vec<CachedModel>,
    pub fetched_at: u64,
}

/// Provider name → its cached entry.
pub type ModelCache = HashMap<String, CachedEntry>;

/// Path to the cache file. Test builds use a temp-dir override.
#[cfg(not(test))]
fn cache_path() -> PathBuf {
    crate::config::profile::base_stemcell_dir().join("startup_models_cache.json")
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
            .unwrap_or_else(|| std::env::temp_dir().join("stemcell-model-cache-unset.json"))
    })
}

#[cfg(test)]
pub(crate) fn set_test_cache_path(path: PathBuf) {
    TEST_CACHE_PATH.with(|p| *p.borrow_mut() = Some(path));
}

/// Load the cache from disk, or an empty cache if missing/corrupt.
///
/// Non-chat models (embedding / image / audio / rerank / …) are filtered out
/// on read so that a cache written by an older build — or any entry that
/// slipped through — never surfaces bloat in the picker.  See
/// [`is_chat_capable_model_id`].
pub fn load() -> ModelCache {
    let mut cache: ModelCache = std::fs::read_to_string(cache_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    for entry in cache.values_mut() {
        entry.models.retain(|m| is_chat_capable_model_id(&m.id));
    }
    cache
}

/// Read the cached model ids for one provider, if present and non-empty.
pub fn models_for(provider: &str) -> Option<Vec<String>> {
    load()
        .get(provider)
        .map(|e| e.models.iter().map(|m| m.id.clone()).collect::<Vec<_>>())
        .filter(|m: &Vec<String>| !m.is_empty())
}

/// Read the full cached model records for one provider, if present and
/// non-empty. Use this when display name, capabilities, cost, or limits are
/// needed; [`models_for`] is the id-only fast path.
pub fn models_full_for(provider: &str) -> Option<Vec<CachedModel>> {
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

/// One-load warm-start lookup: returns the provider's cached model ids (if
/// present and non-empty) and whether the entry is fresh within
/// `max_age_secs`. Folds what would otherwise be back-to-back [`models_for`] +
/// [`is_fresh`] calls into a single disk read + parse on the `/models` open
/// path.
pub fn warm_start(provider: &str, max_age_secs: u64) -> (Option<Vec<String>>, bool) {
    match load().get(provider) {
        Some(e) if !e.models.is_empty() => {
            let fresh = now_epoch().saturating_sub(e.fetched_at) < max_age_secs;
            (Some(e.models.iter().map(|m| m.id.clone()).collect()), fresh)
        }
        _ => (None, false),
    }
}

/// Insert/replace one provider's models from a live API fetch (ids only) and
/// persist. Rich metadata already known for a id (seeded from models.dev) is
/// preserved — the live fetch is authoritative about *which* ids exist, but it
/// carries no capability/cost data of its own. Silently ignores IO errors — a
/// failed write just means `/models` falls back to a live fetch.
///
/// Non-chat ids are filtered out before persisting (see
/// [`is_chat_capable_model_id`]) so a manual Ctrl+R refresh writes a clean,
/// chat-only list back to the cache.
pub fn store(provider: &str, models: Vec<String>) {
    let mut cache = load();
    // Preserve any rich metadata we already hold for these ids.
    let prior: HashMap<String, CachedModel> = cache
        .get(provider)
        .map(|e| e.models.iter().map(|m| (m.id.clone(), m.clone())).collect())
        .unwrap_or_default();
    let models: Vec<CachedModel> = models
        .into_iter()
        .filter(|id| is_chat_capable_model_id(id))
        .map(|id| {
            prior
                .get(&id)
                .cloned()
                .unwrap_or_else(|| CachedModel::id_only(id))
        })
        .collect();
    write_entry(&mut cache, provider, models);
}

/// Insert/replace one provider's models with full models.dev metadata and
/// persist. Non-chat ids are filtered out before persisting.
pub fn store_rich(provider: &str, models: Vec<CachedModel>) {
    let mut cache = load();
    let models: Vec<CachedModel> = models
        .into_iter()
        .filter(|m| is_chat_capable_model_id(&m.id))
        .collect();
    write_entry(&mut cache, provider, models);
}

/// Replace one provider's entry and flush the whole cache to disk.
fn write_entry(cache: &mut ModelCache, provider: &str, models: Vec<CachedModel>) {
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
                models: vec![CachedModel::id_only("opus")],
                fetched_at: 1,
            },
        );
        std::fs::write(&path, serde_json::to_string_pretty(&cache).unwrap()).unwrap();
        assert!(!is_fresh("anthropic", FRESH_TTL_SECS));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn store_preserves_rich_metadata_across_id_only_merge() {
        let dir = std::env::temp_dir().join(format!("oc-model-cache-merge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("startup_models_cache.json");
        let _ = std::fs::remove_file(&path);
        set_test_cache_path(path.clone());

        // models.dev seeds rich metadata.
        store_rich(
            "openai",
            vec![CachedModel {
                id: "gpt-5".into(),
                name: Some("GPT-5".into()),
                tool_call: Some(true),
                reasoning: Some(true),
                input_modalities: vec!["text".into(), "image".into()],
                cost: ModelCost {
                    input: Some(1.25),
                    output: Some(10.0),
                    ..Default::default()
                },
                limit: ModelLimit {
                    context: Some(400_000),
                    output: Some(128_000),
                },
            }],
        );

        // A later live API fetch knows only ids — and adds a new one.
        store("openai", vec!["gpt-5".into(), "gpt-5-mini".into()]);

        let full = models_full_for("openai").unwrap();
        let gpt5 = full.iter().find(|m| m.id == "gpt-5").unwrap();
        // Rich metadata survived the id-only merge.
        assert_eq!(gpt5.name.as_deref(), Some("GPT-5"));
        assert_eq!(gpt5.tool_call, Some(true));
        assert_eq!(gpt5.limit.context, Some(400_000));
        // The newly-discovered id is present but bare.
        let mini = full.iter().find(|m| m.id == "gpt-5-mini").unwrap();
        assert_eq!(mini.name, None);
        assert_eq!(mini.tool_call, None);

        let _ = std::fs::remove_file(&path);
    }
}
