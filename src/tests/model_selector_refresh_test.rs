//! Source-level guards for the `/models` Ctrl+R refresh flow.
//!
//! These pin three regressions:
//! - custom providers must not early-return out of refresh
//! - the success banner must report the raw fetched count
//! - the dialog must render a locked loading state while refreshing

const DIALOGS_SRC: &str = include_str!("../tui/app/dialogs.rs");
const STATE_SRC: &str = include_str!("../tui/app/state.rs");
const RENDER_SRC: &str = include_str!("../tui/render/dialogs.rs");

#[test]
fn refresh_path_fetches_custom_provider_base_url() {
    assert!(
        DIALOGS_SRC.contains("base_url.as_deref()"),
        "manual /models refresh must pass base_url to fetch_provider_models for custom providers"
    );
    assert!(
        !DIALOGS_SRC.contains("if provider_idx >= CUSTOM_PROVIDER_IDX {\n            return;"),
        "manual /models refresh must not bail out for custom providers"
    );
}

#[test]
fn refresh_message_reports_picker_count_and_fetch_delta() {
    assert!(
        STATE_SRC.contains("let fetched_count = models.len();"),
        "ModelSelectorModelsFetched must capture raw endpoint count before merge/fallback"
    );
    assert!(
        STATE_SRC.contains("let picker_total = self.ps.dialog_model_options().len();"),
        "refresh banner must compute the actual picker size after rebuilding the merged options cache"
    );
    assert!(
        STATE_SRC.contains("✓ Picker updated: {} ({} fetched in {})"),
        "refresh banner must headline the picker size and only mention fetched_count as refresh delta"
    );
}

#[test]
fn refreshing_dialog_renders_locked_loading_state() {
    assert!(
        RENDER_SRC.contains("\"  Refreshing model list...\""),
        "model selector dialog must replace list rows with a loading placeholder during refresh"
    );
    assert!(
        RENDER_SRC.contains(
            "let help_text = if app.ps.is_refreshing {\n        vec![(\"[Esc]\", \"Cancel\")]"
        ),
        "refreshing model selector must advertise only Esc/Cancel while input is blocked"
    );
}
