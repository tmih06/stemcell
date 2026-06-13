//! Startup jobs — a queue of work that runs on every TUI boot.
//!
//! Jobs register into a [`StartupJobs`] queue and all run flat-parallel as
//! independent tokio tasks. The runner is spawned non-blocking, so the TUI is
//! interactive while jobs run; a job that fails or panics is logged and
//! recorded but never fatal.
//!
//! Wire-up lives in one place — [`default_jobs`] — and the runner is spawned
//! from `cmd_chat_inner` (the TUI boot path).

pub mod job;
pub mod jobs;
pub mod model_cache;
pub mod registry;

pub use job::{JobOutcome, JobStatus, StartupContext, StartupJob};
pub use registry::StartupJobs;

use std::sync::Arc;

/// Build the queue with all built-in startup jobs registered.
pub fn default_jobs() -> StartupJobs {
    let mut queue = StartupJobs::new();
    queue
        .register(Arc::new(jobs::CheckConfigJob))
        .register(Arc::new(jobs::CheckEnvsJob))
        .register(Arc::new(jobs::ToolsLoadedJob))
        .register(Arc::new(jobs::BrainFilesJob))
        .register(Arc::new(jobs::RsiStatusJob))
        .register(Arc::new(jobs::RsiProposalsJob))
        .register(Arc::new(jobs::RsiDigestJob))
        .register(Arc::new(jobs::FetchModelsJob));
    queue
}

/// Spawn all built-in startup jobs in the background. Returns immediately —
/// the TUI does not wait for jobs to finish. When all jobs complete, a single
/// collapsible [`TuiEvent::StartupInfo`] is emitted: a one-line summary plus a
/// per-job details body (expandable with Ctrl+O).
pub fn spawn(
    config: crate::config::Config,
    pool: crate::db::Pool,
    tools: Vec<String>,
    event_sender: tokio::sync::mpsc::UnboundedSender<crate::tui::events::TuiEvent>,
    ready_tx: tokio::sync::watch::Sender<bool>,
) {
    let ctx = Arc::new(StartupContext {
        config,
        pool: Some(pool),
        tools: Some(tools),
    });
    let queue = default_jobs();
    tokio::spawn(async move {
        let outcomes = queue.run_all(ctx).await;
        let failed = outcomes
            .iter()
            .filter(|o| o.status == JobStatus::Failed)
            .count();
        tracing::info!(
            "[startup] {} job(s) complete, {} failed",
            outcomes.len(),
            failed
        );

        let (summary, details) = render_startup_info(&outcomes, failed);
        let _ = event_sender.send(crate::tui::events::TuiEvent::StartupInfo { summary, details });
        let _ = ready_tx.send(true);
    });
}

/// Build the collapsed one-line summary and the expandable per-job details
/// body from the completed job outcomes.
fn render_startup_info(outcomes: &[JobOutcome], failed: usize) -> (String, String) {
    let total = outcomes.len();
    // The (ctrl+o …) hint is appended by the system-message renderer when a
    // details body is present, so it's intentionally omitted here.
    let summary = if failed == 0 {
        format!("Startup: {total} job(s) ok")
    } else {
        format!("Startup: {failed} of {total} job(s) failed")
    };

    // Stable, readable ordering by job name so the details body doesn't
    // reshuffle between boots just because tasks finished in a different order.
    let mut sorted: Vec<&JobOutcome> = outcomes.iter().collect();
    sorted.sort_by_key(|o| o.name);

    let mut details = String::new();
    for o in sorted {
        let marker = match o.status {
            JobStatus::Ok => "✓",
            JobStatus::Failed => "✗",
        };
        let note = o.message.as_deref().unwrap_or("");
        let sep = if note.is_empty() { "" } else { " — " };
        details.push_str(&format!(
            "{marker} {} ({:?}){sep}{note}\n",
            o.name, o.duration
        ));
    }
    (summary, details.trim_end().to_string())
}
