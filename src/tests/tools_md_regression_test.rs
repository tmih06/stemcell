//! Regression tests for TOOLS.md template to prevent bloat and duplication.
//!
//! These tests enforce that:
//! 1. TOOLS.md stays concise (under 100 lines)
//! 2. No failure timestamps or log entries
//! 3. No content duplicated from BRAIN_PREAMBLE (system prompt)
//! 4. No raw HTML or stack traces

use std::fs;

const TEMPLATE_PATH: &str = "src/docs/reference/templates/TOOLS.md";

fn load_template() -> String {
    fs::read_to_string(TEMPLATE_PATH)
        .unwrap_or_else(|_| panic!("Failed to read {TEMPLATE_PATH}"))
}

/// TOOLS.md must stay under 100 lines.
/// If it grows beyond that, content likely belongs in a skill or on-demand file.
#[test]
fn test_tools_md_line_count() {
    let content = load_template();
    let lines = content.lines().count();
    assert!(
        lines <= 100,
        "TOOLS.md template has {lines} lines (max 100). \
         Move excess content to skills or on-demand .md files."
    );
}

/// TOOLS.md must not contain failure timestamps.
/// These are diagnostic logs, not tool definitions.
/// Pattern: "May \d+", "Jun \d+", "\d+ failures", "Recurring \(" etc.
#[test]
fn test_no_failure_timestamps() {
    let content = load_template();
    let patterns = [
        "failures on",
        "failure:",
        "failure cluster",
        "recurring (",
        "since 202",
        "session id:",
        "timestamp:",
    ];

    for pattern in &patterns {
        let lower = content.to_lowercase();
        assert!(
            !lower.contains(pattern),
            "TOOLS.md contains failure log pattern '{pattern}'. \
             Failure data belongs in feedback_record, not TOOLS.md."
        );
    }
}

/// TOOLS.md must not contain raw HTML or stack traces.
#[test]
fn test_no_raw_html_or_traces() {
    let content = load_template();
    let bad_patterns = ["<!doctype", "<html", "Traceback", "at line \\d+", "panic!("];

    for pattern in &bad_patterns {
        assert!(
            !content.to_lowercase().contains(&pattern.to_lowercase()),
            "TOOLS.md contains raw HTML or trace pattern '{pattern}'."
        );
    }
}

/// TOOLS.md must not duplicate BRAIN_PREAMBLE content.
/// The system prompt already has: search routing, GitHub routing, browser routing,
/// tool parameter list, RSI instructions, plan tool usage.
/// Allow mentions in the "What doesn't belong" boundary section.
#[test]
fn test_no_preamble_duplicates() {
    let content = load_template();

    // Check for actual content sections that duplicate the preamble.
    // We check for section HEADERS, not inline mentions in boundary lists.
    let bad_sections = [
        "## Search Routing",
        "## GitHub Routing",
        "## Browser Routing",
        "## RSI",
        "## Tool Parameters",
        "## Parameter Reference",
    ];

    for section in &bad_sections {
        assert!(
            !content.contains(section),
            "TOOLS.md has section '{section}' which duplicates BRAIN_PREAMBLE content."
        );
    }
}

/// TOOLS.md must not contain duplicate sections.
/// Each header should appear exactly once.
#[test]
fn test_no_duplicate_sections() {
    let content = load_template();
    let mut headers: Vec<&str> = Vec::new();

    for line in content.lines() {
        if line.starts_with("## ") {
            let header = line.trim_start_matches("## ").trim();
            assert!(
                !headers.contains(&header),
                "TOOLS.md has duplicate section: '## {header}'"
            );
            headers.push(header);
        }
    }
}

/// TOOLS.md must not contain full CLI references.
/// These belong in skills (loaded on demand).
#[test]
fn test_no_full_cli_references() {
    let content = load_template();
    let lower = content.to_lowercase();

    // Full CLI references belong in skills, not TOOLS.md
    let cli_ref_patterns = [
        "gh pr list",
        "gh issue list",
        "gh api repos",
        "gog gmail",
        "gog calendar",
        "socialcrabs post",
        "socialcrabs schedule",
    ];

    for pattern in &cli_ref_patterns {
        assert!(
            !lower.contains(pattern),
            "TOOLS.md contains full CLI reference '{pattern}'. \
             Move to a skill and load on demand."
        );
    }
}

/// TOOLS.md must not contain provider configuration guides.
/// These live in config.toml and the onboarding wizard.
/// Allow mentions in the "What doesn't belong" boundary section.
#[test]
fn test_no_provider_config_guides() {
    let content = load_template();
    let lower = content.to_lowercase();

    let config_patterns = [
        "base_url",
        "default_model",
        "[provider]",
    ];

    for pattern in &config_patterns {
        assert!(
            !lower.contains(pattern),
            "TOOLS.md contains provider config pattern '{pattern}'. \
             Provider config lives in config.toml and onboarding."
        );
    }
}

/// TOOLS.md must not contain system commands (macOS/Win/Linux).
/// These are basic OS knowledge.
/// Allow mentions in the "What doesn't belong" boundary section.
#[test]
fn test_no_system_commands() {
    let content = load_template();

    // Only check for actual system command SECTIONS, not boundary mentions
    let bad_headers = [
        "## System Commands",
        "## System",
        "## OS Commands",
        "## macOS",
        "## Windows",
        "## Linux",
    ];

    for header in &bad_headers {
        assert!(
            !content.contains(header),
            "TOOLS.md contains system command section '{header}'. \
             System commands are basic OS knowledge."
        );
    }
}
