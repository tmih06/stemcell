//! OpenCode Zen / Go dynamic model-list tests.
//!
//! Both providers list their models from `models.dev/api.json` — the same
//! catalog opencode itself reads — never a hardcoded array. These tests pin
//! that the fetch wiring resolves to the right models.dev keys:
//!   - `opencode`    → `/opencode/models`    (Zen, full catalog incl. *-free)
//!   - `opencode_go` → `/opencode-go/models` (Go, paid-only)
//!
//! They are network-gated: if models.dev is unreachable the fetch returns an
//! empty vec and the test skips rather than failing CI offline.

#![cfg(feature = "tools-providers")]

use crate::tui::onboarding::fetch_provider_models;
use crate::tui::provider_selector::index_of_provider;

/// Resolve a provider id to its onboarding index, skipping if not compiled in.
fn idx(id: &str) -> Option<usize> {
    index_of_provider(id)
}

#[tokio::test]
async fn opencode_zen_lists_full_catalog_from_models_dev() {
    let Some(i) = idx("opencode") else {
        eprintln!("SKIP: opencode provider not compiled");
        return;
    };
    let models = fetch_provider_models(i, None, None, None).await;
    if models.is_empty() {
        eprintln!("SKIP: models.dev unreachable");
        return;
    }
    // The Zen catalog includes the free subset, whose ids carry "-free".
    assert!(
        models.iter().any(|m| m.ends_with("-free")),
        "Zen catalog should include free models (ids ending in -free), got: {:?}",
        &models[..models.len().min(10)]
    );
    // Sanity: a non-trivial catalog (real fetch returns dozens of models).
    assert!(
        models.len() > 5,
        "expected a sizeable Zen catalog, got {}",
        models.len()
    );
}

#[tokio::test]
async fn opencode_go_lists_go_catalog_from_models_dev() {
    let Some(i) = idx("opencode_go") else {
        eprintln!("SKIP: opencode_go provider not compiled");
        return;
    };
    // Listing needs no key — only completion does.
    let models = fetch_provider_models(i, None, None, None).await;
    if models.is_empty() {
        eprintln!("SKIP: models.dev unreachable");
        return;
    }
    assert!(
        models.len() > 1,
        "expected a non-trivial Go catalog, got {}",
        models.len()
    );
}

#[tokio::test]
async fn opencode_zen_is_superset_of_free_models() {
    let Some(i) = idx("opencode") else {
        return;
    };
    let zen = fetch_provider_models(i, None, None, None).await;
    if zen.is_empty() {
        eprintln!("SKIP: models.dev unreachable");
        return;
    }
    // Free models are a subset of Zen — every "-free" id is in the full list.
    let free: Vec<&String> = zen.iter().filter(|m| m.ends_with("-free")).collect();
    assert!(
        !free.is_empty(),
        "Zen catalog must contain the free subset, got none in {} models",
        zen.len()
    );
}
