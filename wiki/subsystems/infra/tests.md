# Infrastructure — Tests

## Test Strategy

- Inline `#[cfg(test)]` modules per subsystem
- A2A tests use mock Axum server
- Service tests use temporary SQLite databases

## Running

| Command | Scope |
|---------|-------|
| `cargo test --all-features` | Full test suite |
| `cargo test -p stemcell -- a2a` | A2A tests |
| `cargo test -p stemcell -- cron` | Cron tests |
| `cargo test -p stemcell -- rtk` | RTK tests |
| `cargo test -p stemcell -- services` | Service tests |
| `cargo test -p stemcell -- startup` | Startup tests |

## Test Areas

| Area | What's tested |
|------|---------------|
| A2A | JSON-RPC dispatch, SSE streaming, task lifecycle, agent card discovery |
| A2A debate | Bee Colony consensus, confidence weighting, multi-agent coordination |
| Cron | Scheduler wake cycle, job execution, run history recording |
| RTK | Command rewriting, token savings tracking |
| Services | CRUD operations with temp databases, edge cases |
| Startup | Parallel job execution, config/env validation, model cache warm, tools-loaded + brain-files reports (`startup_jobs_test.rs`) |

---

**Navigation:** [Index](index.md) | [Source Map](source-map.md) | [Flows](flows.md)
