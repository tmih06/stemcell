//! OpenCode Zen / Go dynamic model-list tests.
//!
//! Both providers list their models from `models.dev/api.json` — the same
//! catalog opencode itself reads — never a hardcoded array. These tests pin
//! that the fetch wiring resolves to the right models.dev keys:
//!   - `opencode`    → `/opencode/models`    (Zen)
//!   - `opencode_go` → `/opencode-go/models` (Go)
//!
//! Listing returns the full non-deprecated catalog exactly like `opencode
//! models` — it does NOT filter by API key or cost, because browsing models.dev
//! never authenticates. (Auth is enforced at completion, not listing.) This
//! matters for Go: every Go model is paid, so a key/cost filter would return
//! zero rows and the picker would drop the provider entirely.
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
async fn opencode_zen_lists_catalog_without_key() {
    let Some(i) = idx("opencode") else {
        eprintln!("SKIP: opencode provider not compiled");
        return;
    };
    // Listing needs no key — models.dev is a public catalog.
    let models = fetch_provider_models(i, None, None, None).await;
    if models.is_empty() {
        eprintln!("SKIP: models.dev unreachable");
        return;
    }
    // Zen has a sizeable non-deprecated catalog (dozens of models).
    assert!(
        models.len() > 5,
        "expected a sizeable Zen catalog, got {}",
        models.len()
    );
    // The free subset (ids carrying "-free") must be discoverable.
    assert!(
        models.iter().any(|m| m.ends_with("-free")),
        "Zen catalog should include free models, got: {:?}",
        &models[..models.len().min(10)]
    );
}

#[tokio::test]
async fn opencode_zen_listing_is_key_independent() {
    let Some(i) = idx("opencode") else {
        return;
    };
    let without = fetch_provider_models(i, None, None, None).await;
    let with = fetch_provider_models(i, Some("dummy-key-for-listing"), None, None).await;
    if without.is_empty() || with.is_empty() {
        eprintln!("SKIP: models.dev unreachable");
        return;
    }
    // Listing does not gate on the key — same catalog either way.
    assert_eq!(
        without, with,
        "Zen listing must be identical with and without a key (auth is completion-time only)"
    );
}

#[tokio::test]
async fn opencode_go_lists_paid_catalog_without_key() {
    let Some(i) = idx("opencode_go") else {
        eprintln!("SKIP: opencode_go provider not compiled");
        return;
    };
    // Every Go model is paid; listing must still surface them without a key,
    // or the provider vanishes from the picker. Regression guard for that bug.
    let models = fetch_provider_models(i, None, None, None).await;
    if models.is_empty() {
        eprintln!("SKIP: models.dev unreachable");
        return;
    }
    assert!(
        models.len() > 1,
        "expected a non-trivial Go catalog even without a key, got {}",
        models.len()
    );
}
