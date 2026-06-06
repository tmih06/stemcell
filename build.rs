use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::PathBuf;

const TOOL_TOGGLES: &[(&str, &str)] = &[
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
];

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    validate_tool_toggles();

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

fn validate_tool_toggles() {
    let manifest_path =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR missing"))
            .join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", manifest_path.display()));
    let cargo: toml::Value = manifest
        .parse()
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", manifest_path.display()));

    let toggles = cargo
        .get("package")
        .and_then(|v| v.get("metadata"))
        .and_then(|v| v.get("tool_toggles"))
        .and_then(|v| v.as_table())
        .unwrap_or_else(|| panic!("Cargo.toml missing [package.metadata.tool_toggles]"));

    let expected_keys: BTreeSet<&str> = TOOL_TOGGLES.iter().map(|(key, _)| *key).collect();
    let actual_keys: BTreeSet<&str> = toggles.keys().map(|k| k.as_str()).collect();

    let missing: Vec<&str> = expected_keys.difference(&actual_keys).copied().collect();
    let extra: Vec<&str> = actual_keys.difference(&expected_keys).copied().collect();
    let invalid: Vec<&str> = TOOL_TOGGLES
        .iter()
        .filter_map(|(key, _)| {
            toggles
                .get(*key)
                .filter(|v| v.as_bool().is_none())
                .map(|_| *key)
        })
        .collect();

    if !missing.is_empty() || !extra.is_empty() || !invalid.is_empty() {
        panic!(
            "invalid [package.metadata.tool_toggles]: missing={missing:?} extra={extra:?} non_bool={invalid:?}"
        );
    }

    if let Ok(expected_csv) = std::env::var("OPENCRABS_EXPECTED_TOOL_FEATURES") {
        let expected: BTreeSet<String> = expected_csv
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        let active: BTreeSet<String> = TOOL_TOGGLES
            .iter()
            .map(|(_, feature)| *feature)
            .filter(|feature| {
                let env_key = format!("CARGO_FEATURE_{}", feature.replace('-', "_").to_uppercase());
                std::env::var_os(env_key).is_some()
            })
            .map(str::to_string)
            .chain(
                std::iter::once("rtk".to_string())
                    .filter(|_| std::env::var_os("CARGO_FEATURE_RTK").is_some()),
            )
            .collect();

        let expected_toolish: BTreeSet<String> = expected
            .into_iter()
            .filter(|feature| feature == "rtk" || feature.starts_with("tool-"))
            .collect();

        if active != expected_toolish {
            panic!(
                "tool feature mismatch: active={active:?} expected={expected_toolish:?}. \
                 Use the Makefile build/run targets so Cargo features match [package.metadata.tool_toggles]."
            );
        }
    }

    let _toggle_values: HashMap<&str, bool> = TOOL_TOGGLES
        .iter()
        .map(|(key, _)| {
            (
                *key,
                toggles
                    .get(*key)
                    .and_then(|v| v.as_bool())
                    .expect("validated boolean toggle missing"),
            )
        })
        .collect();
}
