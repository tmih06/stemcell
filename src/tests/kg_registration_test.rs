//! Proves the five `kg_*` tools actually land in the live `ToolRegistry` that
//! is handed to the agent/LLM — i.e. that the `KnowledgeGraphModule` is wired
//! into `all_modules()` and enabled by default. This guards against the tools
//! silently disappearing from the model's schema after a refactor/merge.

use crate::brain::tools::modules::{RegistrationMode, register_enabled_tools};
use crate::config::Config;
use crate::db::Database;

const KG_TOOLS: [&str; 5] = ["kg_search", "kg_read", "kg_links", "kg_note", "kg_context"];

async fn registry_tool_names(mode: RegistrationMode) -> Vec<String> {
    let db = Database::connect_in_memory().await.expect("in-memory db");
    db.run_migrations().await.expect("migrations");
    // Every Config field is `#[serde(default)]`, so an empty document yields
    // an all-defaults config — i.e. no `[tools] disabled` entries.
    let config: Config = toml::from_str("").expect("default config");
    let registry = register_enabled_tools(&config, db.pool(), mode);
    registry.list_tools()
}

#[tokio::test]
async fn kg_tools_registered_in_full_mode() {
    let tools = registry_tool_names(RegistrationMode::Full).await;
    for t in KG_TOOLS {
        assert!(
            tools.contains(&t.to_string()),
            "kg tool `{t}` missing from registry. Registered tools: {tools:?}"
        );
    }
}

#[tokio::test]
async fn kg_tools_registered_in_minimal_mode() {
    // The KG module has no Full-mode gate, so it must register in CLI/minimal mode too.
    let tools = registry_tool_names(RegistrationMode::Minimal).await;
    for t in KG_TOOLS {
        assert!(
            tools.contains(&t.to_string()),
            "kg tool `{t}` missing in minimal mode. Registered tools: {tools:?}"
        );
    }
}

#[tokio::test]
async fn kg_tools_exposed_in_llm_definitions() {
    // The schema the provider sends to the model comes from get_tool_definitions();
    // confirm the kg tools are present there too (with non-empty descriptions).
    let db = Database::connect_in_memory().await.expect("db");
    db.run_migrations().await.expect("migrations");
    let config: Config = toml::from_str("").expect("config");
    let registry = register_enabled_tools(&config, db.pool(), RegistrationMode::Full);

    let defs = registry.get_tool_definitions();
    for t in KG_TOOLS {
        let def = defs.iter().find(|d| d.name == t);
        let def = def.unwrap_or_else(|| panic!("kg tool `{t}` absent from LLM tool definitions"));
        assert!(
            !def.description.is_empty(),
            "kg tool `{t}` has an empty description in the LLM schema"
        );
    }
}
