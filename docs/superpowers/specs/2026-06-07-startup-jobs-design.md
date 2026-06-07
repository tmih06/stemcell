# Startup Jobs — Design

**Date:** 2026-06-07
**Status:** Approved (pending written-spec review)

## Summary

A startup job queue that jobs register into and that runs on every TUI boot. Jobs run flat-parallel as independent tokio tasks, non-blocking (the TUI is interactive while they run). A failing or panicking job logs and is recorded in the results but is never fatal.

The first concrete win: warm a model-list cache at boot so the `/models` dialog opens instantly instead of paying a live network fetch each time.

## Scope

**In scope (this implementation):**
- The job-queue framework: `StartupJob` trait, registry, parallel runner.
- Three built-in jobs: check config, check envs, fetch remote models.
- An on-disk model-list cache and wiring it into the `/models` dialog so the latency win is user-visible.

**Out of scope:**
- First-boot / onboarding jobs (a separate concern — not conflated with startup jobs).
- Migrating existing inline startup steps (memory reindex, STT preload, RSI engine) into the queue. They stay where they are; the queue is added alongside.
- Running the queue on utility subcommands (`status`, `version`, `completions`, etc.).

## Execution model (decisions)

- **Trigger:** TUI boot only — the `cmd_chat_inner` path (`opencrabs` no-subcommand, `chat`, `onboard`, `daemon`).
- **Scheduling:** Flat parallel. All registered jobs run concurrently as independent tasks. No inter-job dependencies.
- **Blocking:** Non-blocking. The whole runner is spawned; the TUI becomes interactive immediately and job results surface as they complete.
- **Failure:** Log + record, never fatal. A job returning `Err` or panicking becomes a `Failed` outcome; the app is unaffected.

## Module layout

New module `src/startup/`, sibling to `src/cron/` and `src/services/`:

```
src/startup/
  mod.rs        // re-exports; default_jobs(); run entry
  job.rs        // StartupJob trait, StartupContext, JobOutcome, JobStatus
  registry.rs   // StartupJobs registry + parallel runner (JoinSet)
  jobs/
    mod.rs
    check_config.rs
    check_envs.rs
    fetch_models.rs
```

## Core types (`job.rs`)

```rust
#[async_trait]
pub trait StartupJob: Send + Sync {
    fn name(&self) -> &'static str;
    async fn run(&self, ctx: &StartupContext) -> anyhow::Result<()>;
}

pub enum JobStatus { Ok, Failed }

pub struct JobOutcome {
    pub name: &'static str,
    pub status: JobStatus,
    pub duration: std::time::Duration,
    pub message: Option<String>, // error text on failure
}

pub struct StartupContext {
    pub config: crate::config::Config,
    pub db_pool: crate::db::Pool, // deadpool_sqlite::Pool, from db.pool().clone()
}
```

`StartupContext` carries dependencies explicitly (config + DB pool) rather than reaching for globals, so jobs are testable in isolation. Jobs return `Result<()>`; the runner converts `Err` into a `Failed` outcome and never propagates.

## Registry & runner (`registry.rs`)

```rust
#[derive(Default)]
pub struct StartupJobs {
    jobs: Vec<std::sync::Arc<dyn StartupJob>>,
}

impl StartupJobs {
    pub fn register(&mut self, job: std::sync::Arc<dyn StartupJob>) -> &mut Self;

    /// Spawn every job concurrently, await all, return outcomes.
    pub async fn run_all(self, ctx: std::sync::Arc<StartupContext>) -> Vec<JobOutcome>;
}
```

- Uses `tokio::task::JoinSet`. Each job is spawned with a cloned `Arc<StartupContext>`.
- Each spawned task times itself (`Instant::now()` → `elapsed()`) and produces a `JobOutcome`.
- A panicking job surfaces as a `JoinError`, which the runner converts into a `Failed` outcome — one bad job never takes down the runner or sibling jobs.
- Every outcome is logged: `tracing::info!` with duration on `Ok`, `tracing::warn!` with the message on `Failed`.

## Boot integration (`src/cli/ui.rs`, `cmd_chat_inner`)

After the DB connection and service context exist, add:

```rust
let startup_ctx = std::sync::Arc::new(crate::startup::StartupContext {
    config: config.clone(),
    db_pool: db.pool().clone(),
});
let jobs = crate::startup::default_jobs(); // registers the 3 built-ins
tokio::spawn(jobs.run_all(startup_ctx));
```

`default_jobs()` (in `startup/mod.rs`) is the single place jobs are wired. Existing inline startup steps are left untouched.

## The three jobs

### CheckConfigJob (`jobs/check_config.rs`)
- Calls `ctx.config.validate()` (`src/config/types.rs:3268`).
- Logs any validation issues. No network. Fast.

### CheckEnvsJob (`jobs/check_envs.rs`)
- Verifies the enabled provider has a usable credential (env var or config key) and that expected env vars are well-formed.
- Uses `config.has_any_api_key()` and per-provider key resolution.
- Logs warnings for missing/empty keys. No network. Fast.

### FetchModelsJob (`jobs/fetch_models.rs`)
- For the enabled/configured provider(s), fetches model lists via `fetch_provider_models` (`src/tui/onboarding/fetch.rs:199`) / `fetch_models_from_endpoint` (`src/brain/provider/model_fetch.rs:94`).
- Writes results to an on-disk cache following the `claude_cli_models.json` precedent (`src/brain/provider/claude_cli.rs:22`): a new `startup_models_cache.json` in `base_opencrabs_dir()`.
- Cache shape:
  ```json
  { "<provider_name>": { "models": ["..."], "fetched_at": 1733595600 } }
  ```
- This is the slow, network-bound job — exactly what background execution is for.

## Wiring the cache into `/models`

For the cache to deliver the latency win, the dialog open path must read it:

- In `open_model_selector` (`src/tui/app/dialogs.rs:68`), before/alongside spawning the live fetch (`~line 257`), load `startup_models_cache.json` for the resolved provider and populate `ps.models` synchronously — the user sees cached models instantly rather than an empty list.
- Keep the existing background live fetch so the list refreshes if the cache is stale; when it returns, it overwrites with fresh data via the existing `ModelSelectorModelsFetched` event.

This is the user-visible payoff. Without it, the job warms a cache nothing reads.

## Error handling

- Jobs never panic the process. `Err` → `Failed` outcome; panic → `JoinError` → `Failed` outcome.
- All outcomes logged via `tracing`. No TUI alerts (per the "log + record, never fatal" decision).
- Cache writes ignore IO errors silently (matching `health.rs` / `claude_cli.rs` precedent); a failed cache write just means `/models` falls back to the live fetch.

## Testing

- **registry:** a registry with one passing + one deliberately-failing job returns one `Ok` and one `Failed` outcome and never panics; a panicking job becomes `Failed`.
- **CheckConfigJob / CheckEnvsJob:** constructed `StartupContext` with a temp/sample config; assert outcome and logged behavior.
- **FetchModelsJob:** cache read/write tested against a temp dir; cache round-trip (write → read → matches).
- **/models wiring:** unit test that the dialog open reads cached models into `ps.models` when the cache file exists.

## File-level work summary

- New: `src/startup/{mod,job,registry}.rs`, `src/startup/jobs/{mod,check_config,check_envs,fetch_models}.rs`.
- Edit: `src/lib.rs` (declare `pub mod startup;`), `src/cli/ui.rs` (spawn runner in `cmd_chat_inner`), `src/tui/app/dialogs.rs` (read cache on `open_model_selector`).
