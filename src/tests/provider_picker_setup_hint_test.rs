//! Tests for the /models picker's "show unconfigured providers" behaviour
//! added for issue #126 (B.1).
//!
//! Pre-fix: `configured_providers` filtered out anything without an API
//! key, so Telegram users had no way to see that e.g. OpenCode-API existed
//! as a possible provider until they had already manually edited
//! keys.toml. Post-fix: `all_known_providers_with_status` surfaces every
//! KNOWN_PROVIDERS entry with a `configured: bool` flag so the picker can
//! mark unconfigured ones with a lock emoji and route taps through a
//! `setup:<name>` callback that shows setup instructions.

use crate::config::ProviderConfigs;
use crate::utils::providers::{all_known_providers_with_status, keys_toml_path_hint};

#[test]
fn empty_config_lists_every_known_provider_as_unconfigured() {
    let providers = ProviderConfigs::default();
    let list = all_known_providers_with_status(&providers);

    // Must surface OpenCode (API) — the literal complaint in issue #126.
    let opencode = list.iter().find(|(id, _, _)| id == "opencode");
    assert!(
        opencode.is_some(),
        "issue #126: OpenCode (API) must appear in the picker even without an API key"
    );
    let (_, label, configured) = opencode.unwrap();
    assert!(!*configured);
    assert!(label.contains("OpenCode"));

    // CLI providers must be shown as configured (no API key needed).
    let claude_cli = list.iter().find(|(id, _, _)| id == "claude-cli").unwrap();
    assert!(
        claude_cli.2,
        "CLI providers must be marked configured (no API key required)"
    );
    let opencode_cli = list.iter().find(|(id, _, _)| id == "opencode-cli").unwrap();
    assert!(opencode_cli.2);

    // API providers without a key must be marked unconfigured.
    let anthropic = list.iter().find(|(id, _, _)| id == "anthropic").unwrap();
    assert!(
        !anthropic.2,
        "anthropic without an API key must be marked unconfigured (gives the user a setup hint)"
    );
}

#[test]
fn keys_toml_path_hint_collapses_home_to_tilde() {
    let path = keys_toml_path_hint();
    assert!(
        path.ends_with("keys.toml"),
        "path hint must point at keys.toml, got: {path}"
    );
    // Should NOT contain the raw absolute home — collapsed to ~
    // (per the existing tilde_home util).
    if let Some(home) = std::env::var_os("HOME") {
        let home_str = home.to_string_lossy().to_string();
        if !home_str.is_empty() {
            assert!(
                !path.starts_with(&home_str),
                "path hint must collapse $HOME to ~, got raw path: {path}"
            );
        }
    }
}

#[test]
fn unconfigured_provider_help_text_includes_correct_section() {
    let help = crate::channels::commands::unconfigured_provider_help("opencode");
    assert!(
        help.contains("[providers.opencode]"),
        "help must show the TOML section header, got: {help}"
    );
    assert!(
        help.contains("api_key"),
        "help must show the api_key field, got: {help}"
    );
    assert!(
        help.contains("keys.toml"),
        "help must point at keys.toml, got: {help}"
    );
    // Specifically pin the security warning — bots can't delete user
    // messages in DMs so we explicitly tell users NOT to paste keys in chat.
    assert!(
        help.contains("Do NOT paste"),
        "help must warn against pasting keys in chat (Telegram bots can't delete DMs)"
    );
}

#[test]
fn unconfigured_help_normalizes_hyphenated_provider_to_underscore_section() {
    // opencode-cli → [providers.opencode_cli], not [providers.opencode-cli]
    // (TOML section names match the config struct field names).
    let help = crate::channels::commands::unconfigured_provider_help("opencode-cli");
    assert!(help.contains("[providers.opencode_cli]"));
    assert!(!help.contains("[providers.opencode-cli]"));
}
