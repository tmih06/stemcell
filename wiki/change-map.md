# Change Map

## Adding a New Tool

1. Create tool file in `src/brain/tools/<name>.rs`
2. Implement the `Tool` trait (`src/brain/tools/trait.rs`)
3. Add feature flag in `Cargo.toml` under `[features]` as `tool-<name>`
4. Group under appropriate `tools-*` umbrella feature
5. Register in `src/brain/tools/mod.rs` (module declaration + `register_tools()`)
6. Register in `src/brain/tools/registry.rs` if manual registration needed
7. Add tests in `src/tests/`
8. Update [Source Map](source-map.md), [Coverage Manifest](coverage-manifest.md)

## Adding a New LLM Provider

1. Create file in `src/brain/provider/<name>.rs`
2. Implement the `Provider` trait (`src/brain/provider/trait.rs`)
3. Register in `src/brain/provider/factory.rs` — add variant + instantiation case
4. Add config section in `src/config/types.rs` (if provider-specific settings needed)
5. Add example keys to `keys.toml.example`
6. Add tests
7. Update [Contracts](contracts.md), [Source Map](source-map.md)
8. Reference: `wiki/reference/ADDING_NEW_PROVIDERS.md`

## Adding a New Channel

1. Create directory `src/channels/<name>/`
2. Implement handler, send, and connection logic
3. Optionally add tool files in `src/brain/tools/` for `<name>_connect.rs` and `<name>_send.rs`
4. Register in `src/channels/factory.rs` and `src/channels/manager.rs`
5. Add feature flags in `Cargo.toml`
6. Add per-channel session resolution in `src/channels/session_resolve.rs`
7. Add tests
8. Update [Source Map](source-map.md), [Flows](flows.md)

## Adding a New Migration

1. Create file in `src/migrations/` with timestamp prefix (e.g., `20260611000001_<description>.sql`)
2. Add migration SQL (both up and down if reversible)
3. Update migration test count in `src/db/database.rs` or migration module
4. Add repository methods in `src/db/repository/` if needed
5. Update [Contracts](contracts.md)

## Modifying the Agent Loop

| File | What to inspect |
|------|----------------|
| `src/brain/agent/service/tool_loop.rs` | Core loop orchestration, tool dispatch, error handling |
| `src/brain/agent/service/compaction.rs` | Context compaction thresholds (65% soft, 90% hard) |
| `src/brain/agent/service/context.rs` | Context window management (31KB) |
| `src/brain/agent/service/gaslighting.rs` | System prompt construction |
| `src/brain/agent/service/helpers.rs` | Shared agent helper logic (61KB) |
| `src/brain/agent/service/messaging.rs` | Message formatting & dispatch |
| `src/brain/agent/service/truncation.rs` | Message truncation logic |
| `src/brain/agent/service/types.rs` | Agent service types |

## Coupled Areas & Risky Boundaries

| Boundary | Risk |
|----------|------|
| **Provider fallback chain** (`src/brain/provider/fallback.rs`) | Fallback wrapping/unwrapping in `factory.rs` and session provider swap is fragile. The `is_fallback_chain` / `active_subprovider_name` dance can cause double-wrapping or naked providers. |
| **Tool registry** (`src/brain/tools/registry.rs`) | Every tool must be registered. Missing registration = silent omission from agent. Feature-gated tools must be conditionally registered. |
| **Context compaction** (`src/brain/agent/service/compaction.rs`) | Must stay within model's context window. Soft (65%) and hard (90%) thresholds interact with provider-specific window sizes in `config/types.rs`. |
| **Channel session resolution** (`src/channels/session_resolve.rs`) | Maps channel+chat to session IDs. Errors here cause messages to land in wrong sessions or create duplicates. |
| **Config types** (`src/config/types.rs`) | 126KB struct used by every subsystem. Adding/changing fields requires all callers to be updated. Hot-reload via `src/utils/config_watcher.rs` adds runtime complexity. |
| **Database migrations** (`src/migrations/`) | 24 migrations must be applied in order. Backward-incompatible schema changes break existing installs. |
| **Feature flag matrix** (`Cargo.toml` + `build_toggles.toml` + `build.rs`) | Tool features are per-tool, grouped into umbrella features. The Python resolver (`src/scripts/tool_features.py`) and `build.rs` cross-check must stay in sync. |
