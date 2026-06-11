# Infrastructure — Flows

## A2A Flow

```
HTTP request → Axum server (src/a2a/server.rs)
  → JSON-RPC 2.0 dispatch
  → handler (send / stream / tasks)
  → response or SSE stream
```

## A2A Debate Flow

```
Multiple agents → src/a2a/debate.rs
  → Bee Colony algorithm
  → Confidence-weighted consensus
  → Final response
```

## Cron Flow

```
CronScheduler wake (every 60s) → src/cron/scheduler.rs
  → Poll cron_jobs table
  → Check schedule (due?)
  → Execute job in active session context
  → Record run in cron_job_runs table
```

## RTK Flow

```
Bash command output → src/rtk/rewrite.rs
  → RTK filter applied
  → Rewritten output with reduced tokens
  → src/rtk/tracker.rs — savings recorded
```

## Service Flow

```
ServiceManager::new() → ServiceContext
  → Specific service (SessionService, MessageService, etc.)
  → Repository CRUD operations
```

## Startup Flow

```
main.rs init → StartupJobs::run_all() (src/startup/mod.rs)
  → Parallel job execution (check_config, check_envs, fetch_models, RSI jobs)
  → TUI or CLI continues after jobs complete
```

## Logging Flow

```
Application code → tracing macros (info!, error!, etc.)
  → tracing-subscriber (src/logging/logger.rs)
  ├── Console output (env-filter level)
  └── JSON file output (rotation + cleanup)
```

---

**Navigation:** [Index](index.md) | [Source Map](source-map.md) | [Tests](tests.md)
