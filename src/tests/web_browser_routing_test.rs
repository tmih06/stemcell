//! Sentinel tests for the web / GitHub / browser routing rule.
//!
//! Regression context (2026-05-30): on multiple user sessions the
//! agent reached for `browser_navigate` when asked to "check the
//! GitHub PR" or "look up the docs for X" — the right tools were
//! the `gh` CLI via bash and a search tool respectively. Root
//! cause: the tool descriptions said WHAT each tool did but never
//! WHEN to pick it, and the brain preamble had no routing rule.
//!
//! These tests pin three contracts that together force the routing:
//!
//! 1. `BRAIN_PREAMBLE` carries the WEB / GITHUB / BROWSER ROUTING
//!    block so the rule lands in the system prompt every turn
//!    (not just when the user has loaded `TOOLS.md`).
//! 2. `browser_navigate.description` calls out research / GitHub
//!    as the wrong use case — the model sees this in its tool list.
//! 3. The search tools + `bash` carry positive guidance ("prefer
//!    me for X") so the routing rule has somewhere to land.
//!
//! `browser_navigate.description` is checked via `include_str!` of
//! the source file rather than instantiating the tool — the real
//! constructor needs an `Arc<BrowserManager>`, which costs a real
//! Chrome handshake we don't want in a unit test.

use crate::brain::prompt_builder::build_web_preamble;
use crate::brain::tools::Tool;
use crate::brain::tools::bash::BashTool;
use crate::brain::tools::brave_search::BraveSearchTool;
use crate::brain::tools::exa_search::ExaSearchTool;
use crate::brain::tools::web_search::WebSearchTool;

const BROWSER_NAVIGATE_SRC: &str = include_str!("../brain/tools/browser/navigate.rs");

fn all_web_tools() -> Vec<String> {
    [
        "exa_search",
        "brave_search",
        "web_search",
        "browser_navigate",
        "bash",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

#[test]
fn brain_preamble_carries_routing_block() {
    // With every web surface equipped, the block must include all three
    // surfaces (search / gh / browser) and the "last resort" framing for
    // browser. Without all three, the model still has freedom to misroute.
    let preamble = build_web_preamble(&all_web_tools())
        .expect("preamble must be present when web tools are equipped");
    assert!(
        preamble.contains("WEB / GITHUB / BROWSER ROUTING"),
        "the routing section header must be present in the web preamble \
         so the rule reaches the model every turn"
    );
    assert!(
        preamble.contains("exa_search")
            && preamble.contains("brave_search")
            && preamble.contains("web_search"),
        "preamble must name all three search tools so the model knows \
         the preference order"
    );
    assert!(
        preamble.contains("`gh` CLI"),
        "preamble must name the gh CLI as the GitHub surface"
    );
    assert!(
        preamble.contains("last resort"),
        "browser must be framed as a last resort, not just an option"
    );
}

/// The leak this fix closes: a tool that is NOT equipped must never be
/// name-dropped in the routing prose. When the model sees a tool named in
/// prose but absent from its schema, it reconciles the contradiction by
/// reporting the tool as "disabled" — leaking the existence of tools the
/// user deliberately turned off.
#[test]
fn web_preamble_omits_unequipped_tools() {
    // Only web_search + bash equipped — browser and the other search tools
    // are off. Their names must not appear anywhere in the prose.
    let tools = vec!["web_search".to_string(), "bash".to_string()];
    let preamble =
        build_web_preamble(&tools).expect("preamble present when web_search/bash equipped");
    assert!(
        !preamble.contains("browser_navigate"),
        "unequipped browser_navigate must not be named: {preamble}"
    );
    assert!(
        !preamble.contains("exa_search"),
        "unequipped exa_search must not be named: {preamble}"
    );
    assert!(
        !preamble.contains("brave_search"),
        "unequipped brave_search must not be named: {preamble}"
    );
    assert!(
        preamble.contains("web_search"),
        "equipped web_search must still be named: {preamble}"
    );
}

#[test]
fn web_preamble_absent_when_no_web_tools() {
    let tools = vec!["read_file".to_string(), "edit_file".to_string()];
    assert!(
        build_web_preamble(&tools).is_none(),
        "no web surface equipped → no web preamble at all"
    );
}

#[test]
fn browser_navigate_description_warns_against_research_misuse() {
    // Source-level check — BrowserNavigateTool::new() needs a real
    // BrowserManager so direct instantiation is overkill. The
    // description literal must carry the routing guardrails.
    let src = BROWSER_NAVIGATE_SRC;
    assert!(
        src.contains("DO NOT use for research"),
        "browser_navigate description must explicitly forbid the \
         research misuse pattern"
    );
    assert!(
        src.contains("GitHub"),
        "browser_navigate description must call out GitHub specifically \
         — that's the most common misroute"
    );
    assert!(
        src.contains("exa_search") || src.contains("brave_search") || src.contains("web_search"),
        "browser_navigate description must point at the search \
         alternatives so the model has a place to redirect"
    );
    assert!(
        src.contains("last resort"),
        "browser_navigate must be framed as a last resort, not a default"
    );
}

#[test]
fn web_search_description_positions_as_default() {
    let desc = WebSearchTool.description();
    assert!(
        desc.contains("DEFAULT") || desc.contains("default"),
        "web_search must announce itself as the default research tool: {desc}"
    );
    assert!(
        desc.contains("gh"),
        "web_search must point at the gh CLI for GitHub-specific lookups: {desc}"
    );
}

#[test]
fn exa_search_description_announces_preference_over_web_search() {
    let tool = ExaSearchTool::new(None);
    let desc = tool.description();
    assert!(
        desc.contains("PREFERRED over `web_search`"),
        "exa_search must announce preference over web_search so the \
         model picks it when both are available: {desc}"
    );
}

#[test]
fn brave_search_description_announces_preference_over_web_search() {
    let tool = BraveSearchTool::new("dummy-key".to_string());
    let desc = tool.description();
    assert!(
        desc.contains("PREFERRED over `web_search`"),
        "brave_search must announce preference over web_search: {desc}"
    );
    assert!(
        desc.contains("current events") || desc.contains("news"),
        "brave_search must surface its strength (current events / news) \
         so the model knows when to pick it over exa: {desc}"
    );
}

#[test]
fn bash_description_routes_github_through_gh_cli() {
    let desc = BashTool.description();
    assert!(
        desc.contains("GITHUB OPERATIONS") || desc.contains("gh"),
        "bash must call out the gh CLI as the GitHub surface: {desc}"
    );
    assert!(
        desc.contains("browser_navigate"),
        "bash must explicitly tell the model not to reach for \
         browser_navigate for GitHub: {desc}"
    );
    assert!(
        desc.contains("--json") || desc.contains("structured JSON"),
        "bash must mention gh's structured-JSON output so the model \
         picks --json over scraping: {desc}"
    );
}
