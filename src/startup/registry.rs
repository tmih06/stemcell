//! Startup job registry and parallel runner.
//!
//! Jobs register into a [`StartupJobs`] queue and all run concurrently via a
//! [`tokio::task::JoinSet`]. The runner is non-blocking from the caller's
//! perspective: spawn `run_all` and let it complete in the background while
//! the TUI is already interactive.

use super::job::{JobOutcome, JobStatus, StartupContext, StartupJob};
use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinSet;

/// A queue of startup jobs.
#[derive(Default)]
pub struct StartupJobs {
    jobs: Vec<Arc<dyn StartupJob>>,
}

impl StartupJobs {
    pub fn new() -> Self {
        Self { jobs: Vec::new() }
    }

    /// Register a job into the queue.
    pub fn register(&mut self, job: Arc<dyn StartupJob>) -> &mut Self {
        self.jobs.push(job);
        self
    }

    /// Run every registered job concurrently, await all, and return outcomes.
    ///
    /// A job that returns `Err` becomes a `Failed` outcome. A job whose task
    /// panics surfaces as a `JoinError` and is likewise converted into a
    /// `Failed` outcome — one bad job never takes down the runner or its
    /// siblings. Every outcome is logged.
    pub async fn run_all(self, ctx: Arc<StartupContext>) -> Vec<JobOutcome> {
        let mut set: JoinSet<JobOutcome> = JoinSet::new();
        for job in self.jobs {
            let ctx = ctx.clone();
            set.spawn(async move {
                let name = job.name();
                let start = Instant::now();
                let res = job.run(&ctx).await;
                JobOutcome::from_result(name, res, start.elapsed())
            });
        }

        let mut outcomes = Vec::new();
        while let Some(joined) = set.join_next().await {
            let outcome = match joined {
                Ok(outcome) => outcome,
                Err(e) => {
                    // Task panicked or was cancelled. We can't recover the job
                    // name from a JoinError, so record it generically.
                    JobOutcome::panicked(
                        "<panicked job>",
                        format!("startup job task failed to join: {e}"),
                        std::time::Duration::ZERO,
                    )
                }
            };
            log_outcome(&outcome);
            outcomes.push(outcome);
        }
        outcomes
    }
}

fn log_outcome(outcome: &JobOutcome) {
    match outcome.status {
        JobStatus::Ok => tracing::info!(
            "[startup] job '{}' ok in {:?}{}",
            outcome.name,
            outcome.duration,
            outcome
                .message
                .as_deref()
                .map(|m| format!(": {m}"))
                .unwrap_or_default()
        ),
        JobStatus::Failed => tracing::warn!(
            "[startup] job '{}' failed in {:?}: {}",
            outcome.name,
            outcome.duration,
            outcome.message.as_deref().unwrap_or("(no message)")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::startup::job::StartupJob;
    use async_trait::async_trait;

    fn test_ctx() -> Arc<StartupContext> {
        Arc::new(StartupContext {
            config: crate::config::Config::default(),
            pool: None,
        })
    }

    struct OkJob;
    #[async_trait]
    impl StartupJob for OkJob {
        fn name(&self) -> &'static str {
            "ok-job"
        }
        async fn run(&self, _ctx: &StartupContext) -> anyhow::Result<Option<String>> {
            Ok(Some("did the thing".to_string()))
        }
    }

    struct FailJob;
    #[async_trait]
    impl StartupJob for FailJob {
        fn name(&self) -> &'static str {
            "fail-job"
        }
        async fn run(&self, _ctx: &StartupContext) -> anyhow::Result<Option<String>> {
            anyhow::bail!("deliberate failure")
        }
    }

    struct PanicJob;
    #[async_trait]
    impl StartupJob for PanicJob {
        fn name(&self) -> &'static str {
            "panic-job"
        }
        async fn run(&self, _ctx: &StartupContext) -> anyhow::Result<Option<String>> {
            panic!("deliberate panic")
        }
    }

    #[tokio::test]
    async fn empty_registry_returns_no_outcomes() {
        let jobs = StartupJobs::new();
        let outcomes = jobs.run_all(test_ctx()).await;
        assert!(outcomes.is_empty());
    }

    #[tokio::test]
    async fn ok_and_fail_jobs_both_recorded() {
        let mut jobs = StartupJobs::new();
        jobs.register(Arc::new(OkJob)).register(Arc::new(FailJob));
        let outcomes = jobs.run_all(test_ctx()).await;

        assert_eq!(outcomes.len(), 2);
        let ok = outcomes.iter().find(|o| o.name == "ok-job").unwrap();
        assert_eq!(ok.status, JobStatus::Ok);
        assert_eq!(ok.message.as_deref(), Some("did the thing"));
        let fail = outcomes.iter().find(|o| o.name == "fail-job").unwrap();
        assert_eq!(fail.status, JobStatus::Failed);
        assert!(
            fail.message
                .as_ref()
                .unwrap()
                .contains("deliberate failure")
        );
    }

    #[tokio::test]
    async fn panicking_job_becomes_failed_outcome() {
        let mut jobs = StartupJobs::new();
        jobs.register(Arc::new(PanicJob)).register(Arc::new(OkJob));
        let outcomes = jobs.run_all(test_ctx()).await;

        // Both jobs accounted for; the panic did not abort the runner.
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes.iter().any(|o| o.status == JobStatus::Failed));
        assert!(
            outcomes
                .iter()
                .any(|o| o.name == "ok-job" && o.status == JobStatus::Ok)
        );
    }
}
