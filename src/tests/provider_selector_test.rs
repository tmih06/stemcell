//! Tests for `src/tui/provider_selector.rs` — provider visibility gating and
//! the unified `/models` picker filtering (multi-term, cross-field matching).
//! Relocated out of the source file's inline `#[cfg(test)]` block per project
//! policy (`src/tests/AGENTS.md`).

use crate::tui::provider_selector::{ProviderSelectorState, index_of_provider};

#[test]
fn compiled_cli_provider_visibility_matches_features() {
    #[cfg(feature = "provider-claude-cli")]
    assert!(index_of_provider("claude-cli").is_some());
    #[cfg(not(feature = "provider-claude-cli"))]
    assert!(index_of_provider("claude-cli").is_none());

    #[cfg(feature = "provider-opencode-cli")]
    assert!(index_of_provider("opencode-cli").is_some());
    #[cfg(not(feature = "provider-opencode-cli"))]
    assert!(index_of_provider("opencode-cli").is_none());

    #[cfg(feature = "provider-codex-cli")]
    assert!(index_of_provider("codex-cli").is_some());
    #[cfg(not(feature = "provider-codex-cli"))]
    assert!(index_of_provider("codex-cli").is_none());
}

#[test]
fn dialog_model_options_filter_by_provider_name_and_model_name() {
    let mut state = ProviderSelectorState {
        selected_provider: index_of_provider("openrouter").unwrap_or(0),
        model_filter: "router".to_string(),
        ..Default::default()
    };
    state.models = vec![
        "openai/gpt-4o".to_string(),
        "anthropic/claude-3.7".to_string(),
    ];
    state.rebuild_dialog_model_options_cache();

    let options = state.filtered_dialog_model_options();
    assert!(
        options.iter().any(
            |option| option.provider_name == "OpenRouter" && option.model_id == "openai/gpt-4o"
        )
    );
}

#[test]
fn dialog_model_options_multi_term_matches_all_terms_across_fields() {
    let mut state = ProviderSelectorState {
        selected_provider: index_of_provider("openrouter").unwrap_or(0),
        model_filter: "deepseek free".to_string(),
        ..Default::default()
    };
    state.models = vec![
        "openai/gpt-4o".to_string(),
        "deepseek/deepseek-chat-free".to_string(),
    ];
    state.rebuild_dialog_model_options_cache();

    // Both terms ("deepseek" and "free") appear in the free model id, so it
    // matches; gpt-4o has neither term and is excluded.
    let options = state.filtered_dialog_model_options();
    assert_eq!(
        options.len(),
        1,
        "only the deepseek-free model should match"
    );
    assert_eq!(options[0].model_id, "deepseek/deepseek-chat-free");

    // Terms may also match across different fields: `openrouter` is the
    // provider name while `free` is part of the model id.
    state.model_filter = "openrouter free".to_string();
    let cross_field = state.filtered_dialog_model_options();
    assert_eq!(cross_field.len(), 1);
    assert_eq!(cross_field[0].model_id, "deepseek/deepseek-chat-free");

    // A term that matches nothing rules the option out (AND semantics).
    state.model_filter = "deepseek anthropic".to_string();
    assert!(state.filtered_dialog_model_options().is_empty());
}
