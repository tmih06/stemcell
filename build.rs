use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

/// Maps every recognised pack toggle to the cargo features it
/// enables. Keep in sync with `TOGGLE_TO_FEATURES` in
/// `src/scripts/tool_features.py`.
const TOGGLE_TO_FEATURES: &[(&str, &[&str])] = &[
    // capabilities
    ("local-stt", &["local-stt"]),
    ("local-tts", &["local-tts"]),
    (
        "browser",
        &[
            "browser",
            "tool-browser-navigate",
            "tool-browser-screenshot",
            "tool-browser-click",
            "tool-browser-type",
            "tool-browser-eval",
            "tool-browser-content",
            "tool-browser-wait",
            "tool-browser-find",
            "tool-browser-close",
        ],
    ),
    ("pdfium", &["pdfium"]),
    ("rtk", &["rtk"]),
    ("profiling", &["profiling"]),
    // channels
    (
        "telegram",
        &["telegram", "tool-telegram-connect", "tool-telegram-send"],
    ),
    (
        "whatsapp",
        &["whatsapp", "tool-whatsapp-connect", "tool-whatsapp-send"],
    ),
    (
        "discord",
        &["discord", "tool-discord-connect", "tool-discord-send"],
    ),
    ("slack", &["slack", "tool-slack-connect", "tool-slack-send"]),
    (
        "trello",
        &["trello", "tool-trello-connect", "tool-trello-send"],
    ),
    // file tier
    (
        "file-read",
        &["tool-read", "tool-ls", "tool-glob", "tool-grep"],
    ),
    (
        "file-write",
        &["tool-write", "tool-edit", "tool-hashline-edit"],
    ),
    // bash tier
    ("bash", &["tool-bash"]),
    // search
    (
        "web-search",
        &["tool-web-search", "tool-exa-search", "tool-brave-search"],
    ),
    (
        "memory-search",
        &[
            "tool-memory-search",
            "tool-session-search",
            "tool-channel-search",
        ],
    ),
    // workflow
    (
        "workflow",
        &[
            "tool-task-manager",
            "tool-session-context",
            "tool-plan",
            "tool-http-request",
            "tool-execute-code",
            "tool-notebook-edit",
            "tool-parse-document",
            "tool-config-manager",
            "tool-follow-up-question",
            "tool-cron-manage",
        ],
    ),
    // multi-agent
    (
        "multi-agent",
        &[
            "tool-spawn-agent",
            "tool-wait-agent",
            "tool-send-input",
            "tool-close-agent",
            "tool-resume-agent",
            "tool-team-create",
            "tool-team-delete",
            "tool-team-broadcast",
        ],
    ),
    // rsi
    (
        "rsi",
        &[
            "tool-feedback-record",
            "tool-feedback-analyze",
            "tool-self-improve",
            "tool-rsi-propose",
            "tool-rsi-proposals",
            "tool-tool-manage",
            "tool-rebuild",
            "tool-evolve",
            "tool-dynamic-runtime",
        ],
    ),
    // knowledge graph
    (
        "kg",
        &[
            "tool-kg-search",
            "tool-kg-read",
            "tool-kg-links",
            "tool-kg-note",
            "tool-kg-context",
        ],
    ),
    // image
    (
        "image",
        &[
            "tool-generate-image",
            "tool-analyze-image",
            "tool-analyze-video",
        ],
    ),
    // brain
    (
        "brain",
        &[
            "tool-slash-command",
            "tool-rename-session",
            "tool-load-brain-file",
            "tool-write-stemcell-file",
            "tool-a2a-send",
        ],
    ),
    // providers
    ("claude-cli", &["provider-claude-cli"]),
    ("codex-cli", &["provider-codex-cli"]),
    ("opencode-cli", &["provider-opencode-cli"]),
];

/// Implication rules: enabling `key` also enables every `dep` first
/// (recursively). Keep in sync with `IMPLIES` in
/// `src/scripts/tool_features.py`.
const IMPLIES: &[(&str, &[&str])] = &[("file-write", &["file-read"])];

/// Legacy `tools-*` alias features that source code's
/// `#[cfg(feature = "...")]` gates depend on. Each alias is
/// auto-enabled when ANY of the listed pack keys is on. Keep in
/// sync with `ALIAS_FROM_PACKS` in `src/scripts/tool_features.py`.
const ALIAS_FROM_PACKS: &[(&str, &[&str])] = &[
    ("tools-rsi", &["rsi"]),
    ("tools-dynamic", &["rsi"]),
    ("tools-kg", &["kg"]),
];

fn main() {
    println!("cargo:rerun-if-changed=build_toggles.toml");
    validate_build_toggles();

    // Embed icon and metadata into Windows executables
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("src/assets/icon.ico");
        res.set("ProductName", "StemCell");
        res.set("FileDescription", "StemCell — LLM Harness");
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

fn expand_implies(enabled: &mut BTreeSet<String>) {
    let rules: Vec<(String, Vec<String>)> = IMPLIES
        .iter()
        .map(|(k, deps)| (k.to_string(), deps.iter().map(|s| s.to_string()).collect()))
        .collect();
    loop {
        let mut changed = false;
        for (key, deps) in &rules {
            if enabled.contains(key) {
                for dep in deps {
                    if enabled.insert(dep.clone()) {
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
}

fn resolve_active_features(enabled: &BTreeSet<String>) -> BTreeSet<String> {
    let mut active: BTreeSet<String> = BTreeSet::new();
    for key in enabled {
        if let Some((_, features)) = TOGGLE_TO_FEATURES.iter().find(|(k, _)| *k == key) {
            for feature in *features {
                active.insert((*feature).to_string());
            }
        }
    }
    for (alias, required_packs) in ALIAS_FROM_PACKS {
        if active.contains(*alias) {
            continue;
        }
        if required_packs.iter().any(|p| enabled.contains(*p)) {
            active.insert((*alias).to_string());
        }
    }
    active
}

fn validate_build_toggles() {
    let path = toggle_path();
    let actual = load_toggle_values(&path);

    let expected_keys: BTreeSet<&str> = TOGGLE_TO_FEATURES.iter().map(|(k, _)| *k).collect();
    let actual_keys: BTreeSet<&str> = actual.keys().map(String::as_str).collect();

    let missing: Vec<&str> = expected_keys.difference(&actual_keys).copied().collect();
    let extra: Vec<&str> = actual_keys.difference(&expected_keys).copied().collect();
    if !missing.is_empty() || !extra.is_empty() {
        panic!(
            "invalid build_toggles.toml: missing={missing:?} extra={extra:?}. \
             Add missing keys to the appropriate section, or remove unknown ones."
        );
    }

    if let Ok(expected_csv) = std::env::var("STEMCELL_EXPECTED_FEATURES") {
        let expected: BTreeSet<String> = expected_csv
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        // Recompute what the build should enable from the toggles,
        // applying the same IMPLIES + ALIAS_FROM_PACKS rules as the
        // Python resolver.
        let mut enabled: BTreeSet<String> = actual
            .iter()
            .filter_map(|(k, v)| if *v { Some(k.clone()) } else { None })
            .collect();
        expand_implies(&mut enabled);
        let active = resolve_active_features(&enabled);

        if active != expected {
            panic!(
                "feature mismatch: active={active:?} expected={expected:?}. \
                 Use the Makefile build/run targets so Cargo features match build_toggles.toml."
            );
        }
    }
}
