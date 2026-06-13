# Contracts

## Database Schema

25 migrations in `src/migrations/`:

| Migration | Key tables affected |
|---|---|
| `20251028000001_initial_schema.sql` | Core tables |
| `20251028000002_modernize_schema.sql` | Modernized schema |
| `20251111000001_add_plans.sql` | `plans` |
| `20251113000001_add_plan_enhancements.sql` | Plan enhancements |
| `20260224000001_add_a2a_tasks.sql` | A2A tasks |
| `20260226000001_add_session_provider.sql` | Session provider |
| `20260305000001_add_channel_messages.sql` | Channel messages |
| `20260305000002_add_cron_jobs.sql` | `cron_jobs` |
| `20260306000001_add_usage_ledger.sql` | `usage_ledger` |
| `20260307000001_add_session_working_dir.sql` | `sessions.working_directory` |
| `20260308000001_add_pending_requests.sql` | `pending_requests` |
| `20260330000001_pending_requests_channel_chat_id.sql` | Pending requests channel |
| `20260402000001_add_cron_job_runs.sql` | `cron_job_runs` |
| `20260412000001_add_feedback_ledger.sql` | `feedback_ledger` |
| `20260415000001_add_tool_executions.sql` | `tool_executions` |
| `20260415000002_add_session_category.sql` | `sessions.category` |
| `20260415000003_fix_tool_executions_schema.sql` | Tool executions fix |
| `20260416000001_add_message_input_tokens.sql` | `messages.input_tokens` |
| `20260421000001_add_message_thinking.sql` | `messages.thinking` |
| `20260426000001_add_recent_paths.sql` | `recent_paths` |
| `20260507000001_add_cron_deliver_api_key.sql` | Cron delivery API key |
| `20260517000001_cron_jobs_text_recast.sql` | Cron jobs text recast |
| `20260522000001_add_auto_title_attempted.sql` | `sessions.auto_title_attempted` |
| `20260529000001_add_channel_thread_id.sql` | `channel_thread_id` |
| `20260611000001_add_knowledge_graph.sql` | `kg_note`, `kg_note_fts`, `kg_observation`, `kg_relation` |

Key tables: `sessions`, `messages`, `feedback_ledger`, `tool_executions`, `cron_jobs`, `cron_job_runs`, `usage_ledger`, `plans`, `pending_requests`, `channel_messages`, `recent_paths`, `a2a_tasks`, `kg_note`, `kg_observation`, `kg_relation`.

DB connection: deadpool-sqlite pool via `src/db/database.rs`. Repositories in `src/db/repository/`.

## Knowledge Graph

Migration `20260611000001_add_knowledge_graph.sql`. The vault on disk
(`~/.stemcell/vault/`, overridable via `[memory].vault_dir`) is the source of
truth; these tables are a rebuildable index maintained by `src/brain/kg/sync.rs`.

| Table | Columns |
|---|---|
| `kg_note` | `id`, `path` UNIQUE, `title`, `note_type`, `frontmatter_json`, `checksum`, `mtime`, `size`, `created_at`, `updated_at` |
| `kg_note_fts` | FTS5 virtual table over `note_id` (UNINDEXED), `title`, `body`, `observations`; bm25-ranked |
| `kg_observation` | `id`, `note_id`, `category`, `content`, `tags_json`, `context` |
| `kg_relation` | `id`, `from_id`, `to_id` (NULLABLE — ghost link), `to_name`, `relation_type`, `context` |

`to_id` is nullable so unresolved wikilinks are first-class rows; resolution
back-fills `to_id`. FK clauses are advisory (production omits `PRAGMA
foreign_keys`), so `KnowledgeGraphRepository` deletes child rows explicitly.

Repository: `src/db/repository/knowledge_graph.rs` — `index_note`, `search_fts`,
`neighbors`/`backlinks`, `resolve_dangling_links`/`resolve_links_for_note`,
`prune_paths`, `get_note_by_ref`, `observations_for_note`, `degree`.

### Tool I/O

Tools gated behind `tool-kg-*` features (umbrella `tools-kg`, in `default`):

| Tool | Params | Returns |
|---|---|---|
| `kg_search` | `query`, `n`=5 | Entry points: title · path · snippet · type |
| `kg_read` | `note`, `anchor?`, `section?` | Frontmatter facts + (sliced) body |
| `kg_links` | `note`, `direction`=both (out/in/both) | `relation_type → [[Target]]` lines + backlinks |
| `kg_note` | `title`, `type?`, `observations[]`, `relations[{type,target}]`, `mode`=create/append | Writes/updates note (surgical), reindexes |
| `kg_context` | `query` or `note`, `depth`=1 (max 2), `budget`=12 | Ranked titles + key facts + links |

## Provider Trait

File: `src/brain/provider/trait.rs`

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    async fn complete(&self, request: LLMRequest) -> Result<LLMResponse>;
    async fn stream(&self, request: LLMRequest) -> Result<ProviderStream>;
    fn supports_streaming(&self) -> bool { true }
    fn supports_tools(&self) -> bool { true }
    fn supports_vision(&self) -> bool { false }
    fn cli_handles_tools(&self) -> bool { false }
    fn cli_manages_context(&self) -> bool { self.cli_handles_tools() }
    fn name(&self) -> &str;
    fn base_url(&self) -> Option<&str> { None }
    fn default_model(&self) -> &str;
    fn supported_models(&self) -> Vec<String>;
    fn context_window(&self, model: &str) -> Option<u32>;
    fn configured_context_window(&self) -> Option<u32> { None }
    fn force_next_fallback(&self, _reason: &str) -> bool { false }
    fn take_swap_event(&self) -> Option<SwapEvent> { None }
    fn active_subprovider_name(&self) -> Option<String> { None }
    fn active_subprovider_model(&self) -> Option<String> { None }
    fn is_fallback_chain(&self) -> bool { false }
    fn calculate_cost(&self, model: &str, input_tokens: u32, output_tokens: u32) -> f64;
    fn calculate_cost_with_cache(...) -> f64;
}
```

Key types: `LLMRequest`, `LLMResponse`, `StreamEvent`, `ProviderStream` (all in `src/brain/provider/types.rs`).

## Tool Trait

File: `src/brain/tools/trait.rs`

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> String;
    fn description(&self) -> String;
    fn parameters(&self) -> Value;          // JSON Schema
    async fn run(&self, ctx: ToolExecutionContext, args: Value) -> Result<ToolResult>;
}
```

Key types: `ToolExecutionContext` (session_id, working_directory, env_vars, auto_approve, timeout_secs, sudo_callback), `ToolResult`.

## A2A Protocol

File: `src/a2a/types.rs` — JSON-RPC 2.0 subset (RC v1.0 MVP).

Core types: `Task`, `TaskState` (Submitted, Working, Completed, Failed, Canceled, InputRequired, Rejected, AuthRequired), `Message`, `TaskStatus`, `AgentCard`.

Server: Axum HTTP server at `src/a2a/server.rs`. Endpoint: `POST /rpc`.

## Config Structure

File: `src/config/types.rs` (126KB). Top-level `Config` struct with sections:

| Section | Source |
|---------|--------|
| General settings | `config.toml` |
| Provider configs | `config.toml` (`[providers.*]`) |
| Keys/secrets | `keys.toml` |
| Profiles | `src/config/profile.rs` |
| Crabrace registry | `src/config/crabrace.rs` |

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `DEBUG_LOGS_LOCATION` | Custom log directory |
| `ANTHROPIC_API_KEY` | Anthropic API key |
| `OPENAI_API_KEY` | OpenAI-compatible API key |
| Various provider env vars | Per-provider authentication |

See `config.toml.example` and `keys.toml.example` for the full list.

## Error Types

Errors are defined per subsystem rather than in a single shared module:
- `src/brain/provider/error.rs` — `ProviderError`
- `src/brain/tools/error.rs` — `ToolError`
- `src/brain/agent/error.rs` — `AgentError` (with `format_user_error`)

Cross-cutting code uses `anyhow::Result` and `thiserror`-derived enums.

## Module Organization

File: `src/lib.rs` — declares all public modules: `brain`, `cli`, `config`, `db`, `logging`, `memory`, `services`, `startup`, `tui`, `utils`, `a2a`, `channels`, `cron`, `rtk`, `usage`.
