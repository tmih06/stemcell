//! Tests for the `/debug` slash command's report builder.
//!
//! Context (2026-06-13): while fixing system-prompt leaks there was no way to
//! see what the agent actually boots with. `/debug` dumps the assembled prompt,
//! equipped tools, compiled features, and runtime state. These tests cover the
//! pure `build_debug_report` builder (the `App` handler just gathers state and
//! calls it) plus a guard that `/debug` stays registered in `SLASH_COMMANDS`.

use crate::tui::app::SLASH_COMMANDS;
use crate::tui::app::messaging::{DebugReportInput, DebugSessionInfo, build_debug_report};

fn base_input<'a>(
    tools: &'a [String],
    features: &'a [&'a str],
    disabled: &'a [String],
) -> DebugReportInput<'a> {
    DebugReportInput {
        prompt: Some("You are StemCell, a helpful agent."),
        prompt_tokens: 9,
        provider: "anthropic",
        model: "claude-opus-4-8",
        working_dir: "/home/user/project",
        context_pct: 12.0,
        context_max: 200_000,
        last_input_tokens: Some(24_000),
        tools,
        features,
        disabled,
        inline_files: &[("SOUL.md", "personality"), ("USER.md", "user profile")],
        on_demand_files: &[("MEMORY.md", "long-term memory")],
        brain_dir: "~/.stemcell",
        version: "0.3.35",
        os: "linux",
        session: None,
        rsi_digest: None,
        doctor: "Health Check\nkeys.toml — OK",
    }
}

#[test]
fn report_contains_all_section_headers() {
    let tools = vec!["bash".to_string(), "read_file".to_string()];
    let features = ["telegram", "browser"];
    let disabled: Vec<String> = vec![];
    let report = build_debug_report(&base_input(&tools, &features, &disabled));

    for header in [
        "=== StemCell Debug Report ===",
        "--- Runtime ---",
        "--- Equipped Tools",
        "--- Disabled Modules",
        "--- Compiled Features",
        "--- Brain Files",
        "--- Config Health ---",
        "--- System Prompt",
    ] {
        assert!(
            report.contains(header),
            "report missing section header `{header}`:\n{report}"
        );
    }
}

#[test]
fn report_lists_equipped_tools_sorted() {
    let tools = vec![
        "zebra_tool".to_string(),
        "alpha_tool".to_string(),
        "bash".to_string(),
    ];
    let features: [&str; 0] = [];
    let disabled: Vec<String> = vec![];
    let report = build_debug_report(&base_input(&tools, &features, &disabled));

    let a = report.find("alpha_tool").expect("alpha_tool listed");
    let b = report.find("bash").expect("bash listed");
    let z = report.find("zebra_tool").expect("zebra_tool listed");
    assert!(a < b && b < z, "equipped tools must be sorted:\n{report}");
    assert!(report.contains("--- Equipped Tools (3) ---"));
}

/// A disabled module's name must appear only in the disabled section, never
/// presented as an equipped tool — the same tool-vs-disabled distinction the
/// prompt-leak fixes were about.
#[test]
fn disabled_module_not_listed_as_equipped() {
    let tools = vec!["bash".to_string(), "read_file".to_string()];
    let features: [&str; 0] = [];
    let disabled = vec!["browser".to_string()];
    let report = build_debug_report(&base_input(&tools, &features, &disabled));

    let equipped_section = report
        .split("--- Disabled Modules")
        .next()
        .expect("equipped section precedes disabled section");
    assert!(
        !equipped_section.contains("browser"),
        "disabled module leaked into equipped tools:\n{report}"
    );
    assert!(
        report.contains("--- Disabled Modules (1) ---"),
        "disabled count must show:\n{report}"
    );
}

#[test]
fn empty_tool_list_renders_chatbot_mode() {
    let tools: Vec<String> = vec![];
    let features: [&str; 0] = [];
    let disabled: Vec<String> = vec![];
    let report = build_debug_report(&base_input(&tools, &features, &disabled));
    assert!(report.contains("--- Equipped Tools (0) ---"));
    assert!(
        report.contains("chatbot mode"),
        "empty tools must be framed as chatbot mode:\n{report}"
    );
}

#[test]
fn missing_prompt_renders_placeholder() {
    let tools: Vec<String> = vec![];
    let features: [&str; 0] = [];
    let disabled: Vec<String> = vec![];
    let mut input = base_input(&tools, &features, &disabled);
    input.prompt = None;
    let report = build_debug_report(&input);
    assert!(
        report.contains("not yet assembled"),
        "missing prompt must render a placeholder, not panic:\n{report}"
    );
}

#[test]
fn session_section_present_when_session_set() {
    let tools: Vec<String> = vec![];
    let features: [&str; 0] = [];
    let disabled: Vec<String> = vec![];
    let mut input = base_input(&tools, &features, &disabled);
    let id = uuid::Uuid::new_v4();
    input.session = Some(DebugSessionInfo {
        id,
        title: "My Session",
        model: "claude-opus-4-8",
        token_count: 1234,
        cost: 0.05,
        message_count: 7,
    });
    let report = build_debug_report(&input);
    assert!(report.contains("--- Session ---"));
    assert!(report.contains("My Session"));
    assert!(report.contains(&id.to_string()));
}

#[test]
fn debug_command_registered() {
    assert!(
        SLASH_COMMANDS.iter().any(|c| c.name == "/debug"),
        "/debug must stay registered in SLASH_COMMANDS"
    );
}
