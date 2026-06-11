# Infrastructure

Support subsystems that enable the core LLM harness: A2A, Cron, RTK, Usage, Services, Startup, Error, Logging, Utils.

---

| Subsystem | Path | Purpose |
|-----------|------|---------|
| **A2A** | `src/a2a/` | Agent-to-Agent protocol — Axum HTTP server, JSON-RPC 2.0, SSE streaming, multi-agent debate |
| **Cron** | `src/cron/` | Scheduled job execution — polls DB every 60s, runs within active sessions |
| **RTK** | `src/rtk/` | Rust Token Killer — command rewriting, token savings tracking |
| **Usage** | `src/usage/` | Usage analytics — dashboard (TUI), pricing, auto-categorization |
| **Services** | `src/services/` | Business logic — `ServiceContext`, `ServiceManager`, CRUD services |
| **Startup** | `src/startup/` | Non-blocking parallel startup jobs — config check, model cache warm, RSI |
| **Error** | `src/error/` | `StemCellError` enum with typed `ErrorCode` |
| **Logging** | `src/logging/` | `tracing`-based logger — env-filter, JSON output, file rotation + cleanup |
| **Utils** | `src/utils/` | Shared utilities — sanitization, approval UI, config watcher, retry, etc. |

---

**Navigation:** [Source Map](source-map.md) | [Flows](flows.md) | [Tests](tests.md)
