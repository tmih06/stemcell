# Data Layer

Two subsystems: **Database** (SQLite) and **Memory** (FTS5 + vector embeddings).

---

## Database — `src/db/`

| Component | Description |
|-----------|-------------|
| Connection pool | `deadpool-sqlite` managed pool |
| SQL driver | `rusqlite` |
| Schema management | `rusqlite_migration` (24 timestamped migrations in `src/migrations/`) |
| Data access | Repository pattern in `src/db/repository/` |
| Retry | Exponential backoff via `src/db/retry.rs` |

## Memory — `src/memory/`

| Component | Description |
|-----------|-------------|
| `MemoryEngine` | Three modes: local GGUF, remote API, or FTS5-only |
| Embeddings | `embeddinggemma-300M` GGUF model or API-based |
| FTS5 | Full-text search indexing (`src/memory/index.rs`) |
| Hybrid search | Reciprocal Rank Fusion (RRF) merging FTS5 + vector scores |
| Store | Memory CRUD operations (`src/memory/store.rs`) |

## Migrations — `src/migrations/`

24 files, timestamp-named (`YYYYMMDDHHMMSS_description.sql`), covering:

`initial_schema` → `modernize_schema` → `plans` → `plan_enhancements` → `a2a_tasks` → `session_provider` → `channel_messages` → `cron_jobs` → `usage_ledger` → `session_working_dir` → `pending_requests` → `pending_requests_channel_chat_id` → `cron_job_runs` → `feedback_ledger` → `tool_executions` → `session_category` → `tool_executions_fix` → `message_input_tokens` → `message_thinking` → `recent_paths` → `cron_deliver_api_key` → `cron_jobs_text_recast` → `auto_title_attempted` → `channel_thread_id`

---

**Navigation:** [Source Map](source-map.md) | [Flows](flows.md) | [Tests](tests.md)
