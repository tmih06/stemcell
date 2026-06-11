# src/db/, src/memory/, src/migrations/ — Data Layer

SQLite via `deadpool-sqlite` + `rusqlite`. Home dir resolves via
`crate::config::stemcell_home()` (`~/.stemcell/` or a profile subdir). Main DB at
`~/.stemcell/<db>`; memory DB separately at `~/.stemcell/memory/memory.db`.

## src/db/

**`database.rs`** — `Database` wraps a `Pool` (`= deadpool_sqlite::Pool`).
- `Database::connect(path)` — file DB, `max_size=16`, WAL + `busy_timeout=30s` +
  `synchronous=NORMAL` + 64MB cache (`apply_pragmas`). Sets a `GLOBAL_POOL`
  OnceLock (access via `db::database::global_pool()`) for components without DI.
- `Database::connect_in_memory()` — unique `file:mem_<uuid>?…cache=shared` URI,
  `max_size=1` (serialized; avoids `SQLITE_LOCKED`), `MEMORY` journal. Used by all tests.
- `run_migrations()` — builds `rusqlite_migration::Migrations` from `include_str!`'d
  SQL, runs `to_latest`, then `PRAGMA integrity_check`. Has sqlx-legacy detection.
- Universal access pattern: `pool.get().await?.interact(move |conn| { ... }).await`
  — the closure runs on a blocking thread with a `rusqlite::Connection`.

**`repository/mod.rs`** — defines a generic `Repository<T>` async trait, but most
repos do NOT implement it; they expose bespoke async methods. Re-exports all repos.

**Repository pattern** (canonical: `session.rs`, `recent_paths.rs`):
```rust
#[derive(Clone)] pub struct XRepository { pool: Pool }
impl XRepository {
    pub fn new(pool: Pool) -> Self { Self { pool } }
    pub async fn find(&self, id: i64) -> Result<Option<X>> {
        self.pool.get().await.context(...)?
            .interact(move |conn| conn.prepare_cached(SQL)?
                .query_row(params![id], X::from_row).optional())
            .await.map_err(interact_err)?.context(...)
    }
}
```
Use `prepare_cached`, `params![]`, `.optional()` for nullable single-row. Repos are
constructed ad-hoc: `XRepository::new(db.pool().clone())` — no central registry.

**`models.rs`** — structs matching tables, each with `from_row(&Row) ->
rusqlite::Result<Self>`: `Session`, `Message`, `File`, `Attachment`,
`ToolExecution`, `Plan`, `PlanTask`, `ChannelMessage`, `CronJob`, `CronJobRun`,
`FeedbackEntry`. `retry.rs` = exponential-backoff wrapper for DB ops.

Repos: channel_message, cron_job, cron_job_run, feedback_ledger, file, message,
pending_request, plan, recent_paths, session, tool_execution, usage_ledger.

## src/migrations/ (forward-only SQL)

- Named `YYYYMMDDHHMMSS_description.sql`. Applied in the explicit `vec![]` order in
  `database.rs::run_migrations` — **NOT** filename-sorted at runtime.
- Forward-only (`M::up`, no down). Use `CREATE TABLE IF NOT EXISTS` / `ALTER TABLE
  ADD COLUMN`.

## src/memory/ (FTS5 + vector hybrid search)

Backed by the external `qmd` crate (`Store`, `SearchResult`, `hybrid_search_rrf`).
Separate DB at `~/.stemcell/memory/memory.db`.
- `mod.rs` — `MemoryResult{path,snippet,rank}`. Reads `[memory]` from config.toml
  **fresh each call** (`read_memory_config`). Modes gated by `vector_enabled()`
  (default true) and `embedding_api_configured()`. Collections `"brain"`/`"memory"`,
  default 768 dims.
- `store.rs` — `get_store()` → `&'static Mutex<Store>` (OnceCell singleton).
- `embedding.rs` — lazy GGUF `embeddinggemma-300M` engine; `embed_content`,
  `embed_query_api`, `embed_via_api` (HTTP).
- `index.rs` — `BRAIN_FILES` const list (SOUL.md etc.); `index_file()`, `reindex()`.
- `search.rs` — sanitizes FTS query (each word quoted, implicit AND), embeds query,
  runs `search_fts` + `search_vec`, fuses via `hybrid_search_rrf(..., 60)` (RRF
  k=60). FTS-only fallback with no embedding. All heavy work in `spawn_blocking`.

## Adding a Repository / Table

1. `src/migrations/<ts>_add_foo.sql` — `CREATE TABLE`.
2. Register in `database.rs::run_migrations` `vec!` (**append**, never insert
   mid-array) AND bump `Database::MIGRATION_COUNT` (asserted by
   `test_migrations_idempotent`).
3. Add a `Foo` struct + `from_row` in `src/db/models.rs`.
4. Create `src/db/repository/foo.rs` (`FooRepository`, `new(pool)`, async methods
   using the `pool.get().await?.interact(...)` pattern).
5. Register in `src/db/repository/mod.rs`: `pub mod foo;` + `pub use foo::FooRepository;`.
6. Construct via `FooRepository::new(db.pool().clone())`.
7. Test in `src/tests/foo_repo_test.rs` using `Database::connect_in_memory()`.

## Gotchas

- **Migrations are order-sensitive and count-checked** — append only, bump
  `MIGRATION_COUNT`, no down migrations. Forgetting either breaks startup or the
  idempotency test.
- **In-memory test DB uses `max_size=1`** by design — don't "optimize" it;
  shared-cache concurrency causes `SQLITE_LOCKED`.
- **`global_pool()`** exists only after first `connect`; `None` in pure unit tests.
- **`memory/` reads config from disk each call** (not the in-memory `Config`), so
  it reflects live config.toml.
- Never block in async — DB work goes through `interact` (blocking thread).

## Tests

**New tests go in `src/tests/<area>_test.rs`** (project policy — see
`src/tests/AGENTS.md`), using `Database::connect_in_memory()`. The inline
`#[cfg(test)]` blocks in `db/database.rs`, `db/models.rs`, `db/retry.rs`, repos
(`session/message/channel_message/file/plan.rs`), and `memory/{store,search}.rs`
are existing references — don't extend that pattern. Integration examples in
`src/tests/`: `tool_execution_repo_test.rs`, `phantom_db_persistence_test.rs`,
`session_working_dir_test.rs`, `session_provider_wrap_test.rs`, session/channel
resolve tests. Benches: `src/benches/{database,memory}.rs`. Config & secrets live in
`src/config/` — see `src/config/AGENTS.md`.
