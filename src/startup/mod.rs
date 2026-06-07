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
        .register(Arc::new(jobs::FetchModelsJob));
    queue
}

/// Spawn all built-in startup jobs in the background. Returns immediately —
/// the TUI does not wait for jobs to finish.
pub fn spawn(config: crate::config::Config) {
    let ctx = Arc::new(StartupContext { config });
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
    });
}
