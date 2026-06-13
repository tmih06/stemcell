# Infrastructure — Source Map

## A2A — `src/a2a/`

| File | Purpose |
|------|---------|
| `src/a2a/mod.rs` | Module root |
| `src/a2a/server.rs` | Axum HTTP server — JSON-RPC 2.0 gateway |
| `src/a2a/types.rs` | Protocol type definitions |
| `src/a2a/agent_card.rs` | Agent Card discovery |
| `src/a2a/debate.rs` | Multi-agent debate (Bee Colony) |
| `src/a2a/persistence.rs` | Task persistence |
| `src/a2a/handler/mod.rs` | Handler module |
| `src/a2a/handler/send.rs` | `message/send` handler |
| `src/a2a/handler/stream.rs` | SSE streaming handler |
| `src/a2a/handler/tasks.rs` | `tasks/get`, `tasks/cancel` handlers |
| `src/a2a/test_helpers.rs` | Test utilities |

## Cron — `src/cron/`

| File | Purpose |
|------|---------|
| `mod.rs` | Module root |
| `scheduler.rs` | `CronScheduler` — polls DB every 60s, executes in active session |

## RTK — `src/rtk/`

| File | Purpose |
|------|---------|
| `mod.rs` | Module root — Rust Token Killer integration |
| `rewrite.rs` | Command rewriting via `rtk rewrite` |
| `tracker.rs` | Token savings metrics tracking |

## Usage — `src/usage/`

| File | Purpose |
|------|---------|
| `mod.rs` | Module root |
| `dashboard.rs` | Full-screen TUI dashboard rendering |
| `data.rs` | Usage data queries |
| `cards.rs` | Dashboard card definitions (summary, daily, by project/model/activity, core tools) |
| `categorizer.rs` | Session auto-categorization |
| `pricing.rs` | Pricing table integration |

## Services — `src/services/`

| File | Purpose |
|------|---------|
| `mod.rs` | Module root — business logic |
| `context.rs` | `ServiceContext`, `ServiceManager` |
| `session.rs` | `SessionService` |
| `message.rs` | `MessageService` |
| `file.rs` | `FileService` |
| `plan.rs` | `PlanService` |

## Startup — `src/startup/`

| File | Purpose |
|------|---------|
| `mod.rs` | Startup job runner — non-blocking, parallel execution |
| `job.rs` | `Job` trait |
| `registry.rs` | Jobs registry |
| `model_cache.rs` | Model cache warm job |
| `jobs/mod.rs` | Built-in job re-exports |
| `jobs/check_config.rs` | Config validation |
| `jobs/check_envs.rs` | Environment variable check |
| `jobs/tools_loaded.rs` | Equipped-tool count + names report |
| `jobs/brain_files.rs` | Brain/system files found on disk report |
| `jobs/fetch_models.rs` | Model cache population |
| `jobs/rsi_digest.rs` | RSI daily digest |
| `jobs/rsi_proposals.rs` | RSI proposal loading |
| `jobs/rsi_status.rs` | RSI system status |

## Logging — `src/logging/`

| File | Purpose |
|------|---------|
| `mod.rs` | Module root |
| `logger.rs` | `tracing`-based subscriber — env-filter, JSON formatting, file rotation, log cleanup |

## Utils — `src/utils/`

| File | Purpose |
|------|---------|
| `mod.rs` | Module root |
| `sanitize.rs` | Output sanitization / redaction |
| `approval.rs` | Tool approval UI |
| `file_extract.rs` | File content extraction |
| `image.rs` | Image utilities |
| `pdf_vision.rs` | PDF vision utilities |
| `config_watcher.rs` | Config hot-reload via `notify` |
| `retry.rs` | Generic retry with backoff |
| `string.rs` | String manipulation helpers |
| `git_branch.rs` | Git branch utilities |
| `slack_fmt.rs` | Slack message formatting |
| `providers.rs` | Provider utilities |
| `text_complete.rs` | Text completion helpers |
| `tool_context.rs` | Tool context persistence |
| `fd_suppress.rs` | File descriptor suppression |
| `install.rs` | Installation helpers |

---

**Navigation:** [Index](index.md) | [Flows](flows.md) | [Tests](tests.md)
