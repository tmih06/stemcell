//! Source-level guards for onboarding model refresh behavior.
//!
//! These pin three regressions:
//! - onboarding success must report the raw fetched count
//! - onboarding input must lock the model picker while a refresh is in flight
//! - onboarding render must clear the list and show a loading placeholder

const INPUT_SRC: &str = include_str!("../tui/onboarding/input.rs");
const STATE_SRC: &str = include_str!("../tui/app/state.rs");
const RENDER_SRC: &str = include_str!("../tui/onboarding_render.rs");

#[test]
fn onboarding_refresh_message_uses_raw_fetched_count() {
    assert!(
        STATE_SRC.contains("let fetched_count = models.len();"),
        "OnboardingModelsFetched must capture the raw endpoint count before fallback"
    );
    assert!(
        STATE_SRC.contains("format!(\"✓ Refreshed {} models\", fetched_count)"),
        "onboarding refresh banner must report the raw fetched count without confusing it with merged picker totals"
    );
}

#[test]
fn onboarding_refresh_locks_picker_input() {
    assert!(
        INPUT_SRC.contains(
            "if self.ps.models_fetching\n            && matches!(self.auth_field, AuthField::Model | AuthField::CustomModel)"
        ),
        "onboarding model picker must ignore selection input while refresh is running"
    );
}

#[test]
fn onboarding_refresh_clears_picker_rows() {
    assert!(
        RENDER_SRC.contains("\"  Refreshing model list...\""),
        "onboarding model picker must replace stale rows with a loading placeholder during refresh"
    );
    assert!(
        RENDER_SRC.contains("let model_picker_fetching = step == OnboardingStep::ProviderAuth"),
        "onboarding footer must collapse while the model picker is locked for refresh"
    );
}
