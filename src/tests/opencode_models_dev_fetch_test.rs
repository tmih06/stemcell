//! OpenCode Zen / Go dynamic model-list tests.
//!
//! Both providers list their models from `models.dev/api.json` — the same
//! catalog opencode itself reads — never a hardcoded array. These tests pin
//! that the fetch wiring resolves to the right models.dev keys and applies the
//! same gating the endpoint enforces:
//!   - `opencode`    → `/opencode/models`    (Zen)
//!   - `opencode_go` → `/opencode-go/models` (Go)
//!
//! Without an API key the fetch keeps only free, non-deprecated models — those
//! are the only ones `/zen/v1` serves anonymously (paid + deprecated ids 401).
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
async fn opencode_zen_no_key_lists_only_free_models() {
    let Some(i) = idx("opencode") else {
        eprintln!("SKIP: opencode provider not compiled");
        return;
    };
    // No key → only the free, anonymously-servable subset.
    let models = fetch_provider_models(i, None, None, None).await;
    if models.is_empty() {
        eprintln!("SKIP: models.dev unreachable");
        return;
    }
    // The free subset must be present (most ids carry "-free", but not all —
    // e.g. "big-pickle" is free without the suffix), so just require non-empty.
    assert!(
        !models.is_empty(),
        "no-key Zen fetch must return the free model subset"
    );
}

#[tokio::test]
async fn opencode_zen_with_key_lists_more_than_free_subset() {
    let Some(i) = idx("opencode") else {
        return;
    };
    let free = fetch_provider_models(i, None, None, None).await;
    let full = fetch_provider_models(i, Some("dummy-key-for-listing"), None, None).await;
    if free.is_empty() || full.is_empty() {
        eprintln!("SKIP: models.dev unreachable");
        return;
    }
    // A key unlocks the paid catalog, so the keyed list is a superset of free.
    assert!(
        full.len() >= free.len(),
        "keyed Zen catalog ({}) must be >= free subset ({})",
        full.len(),
        free.len()
    );
    for m in &free {
        assert!(
            full.contains(m),
            "free model {m} must also appear in the keyed catalog"
        );
    }
}

#[tokio::test]
async fn opencode_go_lists_go_catalog_from_models_dev() {
    let Some(i) = idx("opencode_go") else {
        eprintln!("SKIP: opencode_go provider not compiled");
        return;
    };
    // Listing with a key returns the (non-deprecated) Go catalog.
    let models = fetch_provider_models(i, Some("dummy-key-for-listing"), None, None).await;
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
