# Data Layer — Source Map

## Database Core — `src/db/`

| File | Purpose |
|------|---------|
| `database.rs` | `Database` struct, `deadpool-sqlite` pool init, migration runner |
| `mod.rs` | Module re-exports |
| `models.rs` | Rust structs matching SQL table schemas |
| `retry.rs` | DB operation retry with exponential backoff |

## Repositories — `src/db/repository/`

| File | Purpose |
|------|---------|
| `mod.rs` | `Repository` trait + base implementation |
| `session.rs` | Session CRUD |
| `message.rs` | Message CRUD |
| `channel_message.rs` | Channel message CRUD |
| `cron_job.rs` | Cron job CRUD |
| `cron_job_run.rs` | Cron job run history |
| `feedback_ledger.rs` | RSI feedback storage |
| `file.rs` | File metadata storage |
| `pending_request.rs` | Pending approval requests |
| `plan.rs` | Plan CRUD |
| `recent_paths.rs` | Recent file paths tracking |
| `tool_execution.rs` | Tool execution records |
| `usage_ledger.rs` | Usage/pricing records |

## Migrations — `src/migrations/` (24 files)

| File | Description |
|------|-------------|
| `20251028000001_initial_schema.sql` | Initial schema |
| `20251028000002_modernize_schema.sql` | Schema modernization |
| `20251111000001_add_plans.sql` | Plans table |
| `20251113000001_add_plan_enhancements.sql` | Plan enhancements |
| `20260224000001_add_a2a_tasks.sql` | A2A tasks |
| `20260226000001_add_session_provider.sql` | Session provider |
| `20260305000001_add_channel_messages.sql` | Channel messages |
| `20260305000002_add_cron_jobs.sql` | Cron jobs |
| `20260306000001_add_usage_ledger.sql` | Usage ledger |
| `20260307000001_add_session_working_dir.sql` | Session working dir |
| `20260308000001_add_pending_requests.sql` | Pending requests |
| `20260330000001_pending_requests_channel_chat_id.sql` | Channel chat ID on pending requests |
| `20260402000001_add_cron_job_runs.sql` | Cron job runs |
| `20260412000001_add_feedback_ledger.sql` | Feedback ledger |
| `20260415000001_add_tool_executions.sql` | Tool executions |
| `20260415000002_add_session_category.sql` | Session category |
| `20260415000003_fix_tool_executions_schema.sql` | Tool execution schema fix |
| `20260416000001_add_message_input_tokens.sql` | Message input tokens |
| `20260421000001_add_message_thinking.sql` | Message thinking field |
| `20260426000001_add_recent_paths.sql` | Recent paths |
| `20260507000001_add_cron_deliver_api_key.sql` | Cron deliver API key |
| `20260517000001_cron_jobs_text_recast.sql` | Cron jobs text type recast |
| `20260522000001_add_auto_title_attempted.sql` | Auto-title attempted flag |
| `20260529000001_add_channel_thread_id.sql` | Channel thread ID |

## Memory — `src/memory/`

| File | Purpose |
|------|---------|
| `mod.rs` | `MemoryEngine` (local / API / FTS5-only modes) |
| `embedding.rs` | Embedding engine — local GGUF (`embeddinggemma-300M`) or API |
| `index.rs` | FTS5 indexing |
| `search.rs` | Hybrid search (RRF: FTS5 + vector score fusion) |
| `store.rs` | Memory store operations |

## Benches — `src/benches/`

| File | Purpose |
|------|---------|
| `database.rs` | DB benchmark suite |
| `memory.rs` | Memory search benchmark suite |

---

**Navigation:** [Index](index.md) | [Flows](flows.md) | [Tests](tests.md)
