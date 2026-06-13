//! Tests for the `tools-loaded` and `brain-files` startup jobs.
//!
//! Context (2026-06-13): the startup-info report listed config/env/RSI jobs but
//! nothing about the agent's actual equipment — how many tools registered or
//! which system files loaded. These two jobs surface that. The tools-loaded job
//! reports the *live* equipped-tool list (passed via `StartupContext::tools`),
//! not compiled features, so a tool that isn't equipped never appears.

use crate::startup::job::{JobStatus, StartupContext, StartupJob};
use crate::startup::jobs::{BrainFilesJob, ToolsLoadedJob};

fn ctx_with_tools(tools: Option<Vec<String>>) -> StartupContext {
    StartupContext {
        config: crate::config::Config::default(),
        pool: None,
        tools,
    }
}

// ── tools-loaded ────────────────────────────────────────────────────────────

#[tokio::test]
async fn tools_loaded_reports_count_and_names() {
    let tools = vec![
        "bash".to_string(),
        "read_file".to_string(),
        "edit_file".to_string(),
    ];
    let ctx = ctx_with_tools(Some(tools));
    let note = ToolsLoadedJob
        .run(&ctx)
        .await
        .expect("job must not fail")
        .expect("job must produce a note");
    assert!(note.contains('3'), "count must be in the note: {note}");
    assert!(note.contains("bash"), "tool names must be listed: {note}");
    assert!(
        note.contains("read_file"),
        "tool names must be listed: {note}"
    );
}

#[tokio::test]
async fn tools_loaded_names_are_sorted() {
    let tools = vec![
        "zebra_tool".to_string(),
        "alpha_tool".to_string(),
        "mango_tool".to_string(),
    ];
    let ctx = ctx_with_tools(Some(tools));
    let note = ToolsLoadedJob.run(&ctx).await.unwrap().unwrap();
    let a = note.find("alpha_tool").unwrap();
    let m = note.find("mango_tool").unwrap();
    let z = note.find("zebra_tool").unwrap();
    assert!(a < m && m < z, "tool names must be sorted: {note}");
}

#[tokio::test]
async fn tools_loaded_handles_chatbot_mode() {
    // Empty tool list = chatbot mode (all tools disabled). The job must say so
    // explicitly rather than emitting a bare "0 tools:".
    let ctx = ctx_with_tools(Some(vec![]));
    let note = ToolsLoadedJob.run(&ctx).await.unwrap().unwrap();
    assert!(note.contains('0'), "must report zero: {note}");
    assert!(
        note.to_lowercase().contains("chatbot"),
        "empty list must be framed as chatbot mode: {note}"
    );
}

#[tokio::test]
async fn tools_loaded_skips_when_no_registry() {
    // No tools field (e.g. a context built without a registry) → graceful skip,
    // never a panic or a misleading "0 tools".
    let ctx = ctx_with_tools(None);
    let note = ToolsLoadedJob.run(&ctx).await.unwrap().unwrap();
    assert!(
        note.to_lowercase().contains("skip"),
        "missing registry must skip: {note}"
    );
}

#[tokio::test]
async fn tools_loaded_never_fails() {
    // Non-fatal contract: the job must always return Ok so a flaky tool list
    // can't abort the boot report.
    let ctx = ctx_with_tools(Some(vec!["bash".to_string()]));
    assert_eq!(
        crate::startup::JobOutcome::from_result(
            ToolsLoadedJob.name(),
            ToolsLoadedJob.run(&ctx).await,
            std::time::Duration::ZERO,
        )
        .status,
        JobStatus::Ok
    );
}

// ── brain-files ───────────────────────────────────────────────────────────

#[tokio::test]
async fn brain_files_job_runs_and_reports() {
    // Reads the real brain dir (~/.stemcell). We don't assert on specific files
    // — the dir's contents vary per machine — only that the job completes Ok and
    // produces a non-empty note. The job is read-only.
    let ctx = ctx_with_tools(None);
    let outcome = crate::startup::JobOutcome::from_result(
        BrainFilesJob.name(),
        BrainFilesJob.run(&ctx).await,
        std::time::Duration::ZERO,
    );
    assert_eq!(outcome.status, JobStatus::Ok, "brain-files must not fail");
    assert!(
        outcome.message.is_some_and(|m| !m.is_empty()),
        "brain-files must produce a note"
    );
}

#[tokio::test]
async fn brain_files_job_name_is_stable() {
    assert_eq!(BrainFilesJob.name(), "brain-files");
    assert_eq!(ToolsLoadedJob.name(), "tools-loaded");
}

// ── registry wiring ─────────────────────────────────────────────────────────

#[test]
fn default_jobs_includes_new_jobs() {
    // The two new jobs must be wired into the default queue, else they never
    // run on boot. We can't introspect job names from the queue directly, so we
    // assert the count grew to include them (6 prior + 2 new = 8).
    let queue = crate::startup::default_jobs();
    assert_eq!(
        queue.len(),
        8,
        "default queue must register all built-in jobs including tools-loaded \
         and brain-files"
    );
}
