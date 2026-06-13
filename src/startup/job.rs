//! Core types for the startup job queue.
//!
//! A [`StartupJob`] is a unit of work that runs on every TUI boot. Jobs run
//! flat-parallel as independent tasks, are non-blocking (the TUI is already
//! interactive while they run), and are never fatal: a job that returns `Err`
//! or panics is recorded as a [`JobStatus::Failed`] outcome and logged, but
//! the app is unaffected.

use async_trait::async_trait;
use std::time::Duration;

/// Shared context handed to every startup job.
///
/// Carries dependencies explicitly (rather than reaching for globals) so jobs
/// can be constructed and tested in isolation.
#[derive(Clone)]
pub struct StartupContext {
    pub config: crate::config::Config,
    /// Database pool, for jobs that read/write app state (e.g. the RSI
    /// feedback digest). `None` in unit tests that don't need it.
    pub pool: Option<crate::db::Pool>,
    /// The equipped tool names from the live registry, captured at spawn time.
    /// `None` when the registry isn't available (unit tests, headless paths
    /// that skip tool registration).
    pub tools: Option<Vec<String>>,
}

/// A unit of startup work.
#[async_trait]
pub trait StartupJob: Send + Sync {
    /// Stable, human-readable name used in logs and outcomes.
    fn name(&self) -> &'static str;

    /// Run the job. Returning `Err` is non-fatal — it becomes a `Failed`
    /// outcome that is logged but never propagated.
    ///
    /// `Ok(Some(note))` carries a one-line, human-readable summary of what the
    /// job did (e.g. `"cached 42 models"`); it is surfaced in the collapsible
    /// startup-info line. `Ok(None)` means "nothing worth reporting".
    async fn run(&self, ctx: &StartupContext) -> anyhow::Result<Option<String>>;
}

/// Terminal status of a job run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Ok,
    Failed,
}

/// The result of running a single startup job.
#[derive(Debug, Clone)]
pub struct JobOutcome {
    pub name: &'static str,
    pub status: JobStatus,
    pub duration: Duration,
    /// On `Ok`, a one-line report note (if the job produced one). On `Failed`,
    /// the error text.
    pub message: Option<String>,
}

impl JobOutcome {
    /// Build an outcome from a job's `Result` and elapsed time.
    pub fn from_result(
        name: &'static str,
        res: anyhow::Result<Option<String>>,
        duration: Duration,
    ) -> Self {
        match res {
            Ok(note) => Self {
                name,
                status: JobStatus::Ok,
                duration,
                message: note,
            },
            Err(e) => Self {
                name,
                status: JobStatus::Failed,
                duration,
                message: Some(format!("{e:#}")),
            },
        }
    }

    /// Build a `Failed` outcome for a job whose task panicked (JoinError).
    pub fn panicked(name: &'static str, message: String, duration: Duration) -> Self {
        Self {
            name,
            status: JobStatus::Failed,
            duration,
            message: Some(message),
        }
    }
}
