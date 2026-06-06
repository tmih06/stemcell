use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

/// Maps every recognised toggle key to the single cargo feature it
/// enables. Keep in sync with `TOGGLE_TO_FEATURE` in
/// `src/scripts/tool_features.py`.
const TOGGLE_TO_FEATURE: &[(&str, &str)] = &[
    // tools
    ("read_file", "tool-read"),
    ("write_file", "tool-write"),
    ("edit_file", "tool-edit"),
    ("hashline_edit", "tool-hashline-edit"),
    ("bash", "tool-bash"),
    ("ls", "tool-ls"),
    ("glob", "tool-glob"),
    ("grep", "tool-grep"),
    ("web_search", "tool-web-search"),
    ("memory_search", "tool-memory-search"),
    ("session_search", "tool-session-search"),
    ("channel_search", "tool-channel-search"),
    ("exa_search", "tool-exa-search"),
    ("brave_search", "tool-brave-search"),
    ("task_manager", "tool-task-manager"),
    ("session_context", "tool-session-context"),
    ("http_request", "tool-http-request"),
    ("plan", "tool-plan"),
    ("execute_code", "tool-execute-code"),
    ("notebook_edit", "tool-notebook-edit"),
    ("parse_document", "tool-parse-document"),
    ("config_manager", "tool-config-manager"),
    ("follow_up_question", "tool-follow-up-question"),
    ("cron_manage", "tool-cron-manage"),
    ("spawn_agent", "tool-spawn-agent"),
    ("wait_agent", "tool-wait-agent"),
    ("send_input", "tool-send-input"),
    ("close_agent", "tool-close-agent"),
    ("resume_agent", "tool-resume-agent"),
    ("team_create", "tool-team-create"),
    ("team_delete", "tool-team-delete"),
    ("team_broadcast", "tool-team-broadcast"),
    ("feedback_record", "tool-feedback-record"),
    ("feedback_analyze", "tool-feedback-analyze"),
    ("self_improve", "tool-self-improve"),
    ("rsi_propose", "tool-rsi-propose"),
    ("generate_image", "tool-generate-image"),
    ("analyze_image", "tool-analyze-image"),
    ("analyze_video", "tool-analyze-video"),
    ("slash_command", "tool-slash-command"),
    ("rename_session", "tool-rename-session"),
    ("load_brain_file", "tool-load-brain-file"),
    ("write_opencrabs_file", "tool-write-opencrabs-file"),
    ("a2a_send", "tool-a2a-send"),
    ("telegram_connect", "tool-telegram-connect"),
    ("telegram_send", "tool-telegram-send"),
    ("whatsapp_connect", "tool-whatsapp-connect"),
    ("whatsapp_send", "tool-whatsapp-send"),
    ("discord_connect", "tool-discord-connect"),
    ("discord_send", "tool-discord-send"),
    ("slack_connect", "tool-slack-connect"),
    ("slack_send", "tool-slack-send"),
    ("trello_connect", "tool-trello-connect"),
    ("trello_send", "tool-trello-send"),
    ("browser_navigate", "tool-browser-navigate"),
    ("browser_screenshot", "tool-browser-screenshot"),
    ("browser_click", "tool-browser-click"),
    ("browser_type", "tool-browser-type"),
    ("browser_eval", "tool-browser-eval"),
    ("browser_content", "tool-browser-content"),
    ("browser_wait", "tool-browser-wait"),
    ("browser_find", "tool-browser-find"),
    ("browser_close", "tool-browser-close"),
    ("rebuild", "tool-rebuild"),
    ("evolve", "tool-evolve"),
    ("tool_manage", "tool-tool-manage"),
    ("rsi_proposals", "tool-rsi-proposals"),
    ("dynamic_runtime", "tool-dynamic-runtime"),
    // channels
    ("telegram", "telegram"),
    ("whatsapp", "whatsapp"),
    ("discord", "discord"),
    ("slack", "slack"),
    ("trello", "trello"),
    // capabilities
    ("local-stt", "local-stt"),
    ("local-tts", "local-tts"),
    ("browser", "browser"),
    ("pdfium", "pdfium"),
    ("rtk", "rtk"),
];

/// Coarse `tools-*` alias features that legacy
/// `#[cfg(feature = "tools-rsi")]`-style gates in the source depend
/// on. Each alias is auto-enabled when any of its sub-tools is on, so
/// those gates keep working even though the user-facing toggle is the
/// per-tool one. Keep in sync with `ALIAS_SUB_TOOLS` in
/// `src/scripts/tool_features.py`.
const ALIAS_SUB_TOOLS: &[(&str, &[&str])] = &[
    (
        "tools-rsi",
        &[
            "tool-feedback-record",
            "tool-feedback-analyze",
            "tool-self-improve",
            "tool-rsi-propose",
        ],
    ),
    (
        "tools-dynamic",
        &[
            "tool-tool-manage",
            "tool-rsi-proposals",
            "tool-dynamic-runtime",
        ],
    ),
];

fn main() {
    println!("cargo:rerun-if-changed=build_toggles.toml");
    validate_build_toggles();

    // Embed icon and metadata into Windows executables
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("src/assets/icon.ico");
        res.set("ProductName", "OpenCrabs");
        res.set("FileDescription", "OpenCrabs — AI Agent");
        res.compile().expect("Failed to compile Windows resources");
    }
}

fn toggle_path() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR missing"))
        .join("build_toggles.toml")
}

fn load_toggle_values(path: &PathBuf) -> BTreeMap<String, bool> {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let parsed: toml::Value = raw
        .parse()
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()));

    let table = parsed
        .as_table()
        .unwrap_or_else(|| panic!("{}: top level must be a TOML table", path.display()));

    let mut flat: BTreeMap<String, bool> = BTreeMap::new();
    for (section, entries) in table {
        let entries = entries.as_table().unwrap_or_else(|| {
            panic!(
                "{}: [{}] must be a table of booleans",
                path.display(),
                section
            )
        });
        for (key, value) in entries {
            let value = value.as_bool().unwrap_or_else(|| {
                panic!(
                    "{}: [{}.{}] must be a boolean",
                    path.display(),
                    section,
                    key
                )
            });
            flat.insert(key.clone(), value);
        }
    }
    flat
}

fn validate_build_toggles() {
    let path = toggle_path();
    let actual = load_toggle_values(&path);

    let expected_keys: BTreeSet<&str> = TOGGLE_TO_FEATURE.iter().map(|(k, _)| *k).collect();
    let actual_keys: BTreeSet<&str> = actual.keys().map(String::as_str).collect();

    let missing: Vec<&str> = expected_keys.difference(&actual_keys).copied().collect();
    let extra: Vec<&str> = actual_keys.difference(&expected_keys).copied().collect();
    if !missing.is_empty() || !extra.is_empty() {
        panic!(
            "invalid build_toggles.toml: missing={missing:?} extra={extra:?}. \
             Add missing keys to the appropriate section, or remove unknown ones."
        );
    }

    if let Ok(expected_csv) = std::env::var("OPENCRABS_EXPECTED_FEATURES") {
        let expected: BTreeSet<String> = expected_csv
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        let mut active: BTreeSet<String> = TOGGLE_TO_FEATURE
            .iter()
            .map(|(_, feature)| *feature)
            .filter(|feature| {
                let env_key = format!("CARGO_FEATURE_{}", feature.replace('-', "_").to_uppercase());
                std::env::var_os(env_key).is_some()
            })
            .map(str::to_string)
            .collect();

        // Mirror the resolver's auto-alias behaviour so the cross-check
        // accounts for `tools-rsi` / `tools-dynamic` being implicitly
        // enabled when one of their sub-tools is on.
        for (alias, sub_tools) in ALIAS_SUB_TOOLS {
            let env_key = format!("CARGO_FEATURE_{}", alias.replace('-', "_").to_uppercase());
            if std::env::var_os(env_key).is_some() {
                active.insert((*alias).to_string());
            } else if sub_tools.iter().any(|st| active.contains(*st)) {
                // Alias wasn't passed to Cargo but the source compiled
                // it anyway — treat it as active for the cross-check.
                active.insert((*alias).to_string());
            }
        }

        if active != expected {
            panic!(
                "feature mismatch: active={active:?} expected={expected:?}. \
                 Use the Makefile build/run targets so Cargo features match build_toggles.toml."
            );
        }
    }
}
