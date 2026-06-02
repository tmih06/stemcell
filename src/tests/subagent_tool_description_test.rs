//! Sentinel: every sub-agent spawn/resume tool description must
//! mention `subagent_provider` / `subagent_model` so the LLM (and
//! anyone reading the tool schema) knows how to control the child
//! agent's provider and model.
//!
//! Context (issue #152, 2026-06-02): a user reported that
//! `spawn_agent` had "no way to specify which model a sub-agent
//! should use — it always inherits the parent session's model." The
//! claim was wrong on its face — `config.agent.subagent_provider`
//! and `config.agent.subagent_model` are read by spawn.rs:120-130,
//! resume.rs:130-134, and team/create.rs:122-162 to override
//! the parent's provider for every spawned child. But none of the
//! tool descriptions said so, so the LLM (which only sees the tool
//! schema, never the README) genuinely had no way to answer "how do
//! I run sub-agents on a different model?" from its prompt context
//! alone.
//!
//! These tests pin the config-mention in each tool description so a
//! future trim of the description text can't silently re-open the
//! gap and re-trigger the same misunderstanding.

use crate::brain::tools::Tool;
use crate::brain::tools::subagent::SpawnAgentTool;

#[test]
fn spawn_agent_description_mentions_subagent_provider_config() {
    let tool = SpawnAgentTool::new(
        std::sync::Arc::new(crate::brain::tools::subagent::SubAgentManager::new()),
        std::sync::Arc::new(crate::brain::tools::ToolRegistry::new()),
    );
    let desc = tool.description();
    assert!(
        desc.contains("subagent_provider"),
        "spawn_agent description must name `subagent_provider` so the LLM knows the \
         config key to point users at. Without this, the model concludes there's no \
         way to control child-agent provider selection (issue #152). \
         \nCurrent description: {desc}"
    );
    assert!(
        desc.contains("subagent_model"),
        "spawn_agent description must also name `subagent_model` so the model key \
         is discoverable from the same schema."
    );
}

#[test]
fn resume_agent_description_mentions_subagent_provider_config() {
    use crate::brain::tools::subagent::ResumeAgentTool;
    let tool = ResumeAgentTool::new(
        std::sync::Arc::new(crate::brain::tools::subagent::SubAgentManager::new()),
        std::sync::Arc::new(crate::brain::tools::ToolRegistry::new()),
    );
    let desc = tool.description();
    assert!(
        desc.contains("subagent_provider") && desc.contains("subagent_model"),
        "resume_agent description must name both config keys so the LLM knows \
         resume follows the same routing rule as spawn. \nCurrent description: {desc}"
    );
}

#[test]
fn team_create_description_mentions_subagent_provider_config() {
    use crate::brain::tools::subagent::{TeamCreateTool, TeamManager};
    let tool = TeamCreateTool::new(
        std::sync::Arc::new(crate::brain::tools::subagent::SubAgentManager::new()),
        std::sync::Arc::new(TeamManager::new()),
        std::sync::Arc::new(crate::brain::tools::ToolRegistry::new()),
    );
    let desc = tool.description();
    assert!(
        desc.contains("subagent_provider") && desc.contains("subagent_model"),
        "team_create description must name both config keys so the LLM knows \
         every team member follows the same routing rule. \nCurrent description: {desc}"
    );
}

#[test]
fn readme_documents_subagent_provider_and_model_keys() {
    // Bundled at compile time so the test is hermetic and a doc tidy-up
    // that drops the section can't slip past CI.
    const README: &str = include_str!("../../README.md");
    assert!(
        README.contains("subagent_provider"),
        "README must document `subagent_provider` so users searching the docs \
         find the config key. Issue #152 came from a user who didn't know it existed."
    );
    assert!(
        README.contains("subagent_model"),
        "README must document `subagent_model` alongside subagent_provider."
    );
}
