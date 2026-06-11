# src/config/ — Configuration & Secrets

Config loads from `~/.stemcell/` (or a profile subdir). `types.rs` is **126KB / 3400+
lines** — never read it whole; use `grep -n "pub struct"` or line-offset reads.

## Files

- **`types.rs`** — top-level `Config` struct (fields: `crabrace, database, logging,
  debug, providers, channels, agent, daemon, image, cron, memory, brain, tools,
  statusline`) + ~50 sub-structs (per-channel configs, `MemoryConfig`,
  `EmbeddingConfig`, `ProviderConfig`, `VoiceConfig`, …).
  - `Config::load()` → `load_inner()`: defaults → merge system config.toml → merge
    local config.toml → `migrate_if_needed` → merge `keys.toml` (overrides). Wrapped
    with last-known-good recovery (`load_last_good_config()`, `Config::was_recovered()`).
    `CONFIG_LOAD_LOCK` Mutex guards read-modify-write.
  - Key fns: `stemcell_home()`, `keys_path()`, `system_config_path()` /
    `local_config_path()`, `Config::validate()`, `write_key` / `write_keys_key` /
    `write_secret_key` (targeted TOML edits preserving structure), `save_keys`,
    `save_last_good_config` / `daily_backup`, `merge_provider_keys`.
  - Defaults via `#[serde(default = "fn")]` + `impl Default`. Unknown top-level keys
    tracked for typo warnings.
- **`secrets.rs`** — `SecretString` (zeroize-on-drop): `new` / `from_str` /
  `expose_secret()`. `Debug`/`Display`/`Serialize` all emit `[REDACTED]` (secrets
  never serialize back to disk); `Deserialize` reads raw. Use for **any** API
  key/token field.
- **`profile.rs`** — `ProfileRegistry` (multi-instance): `set_active_profile` /
  `active_profile` / `resolve_profile_home()`, `create/delete/list/export/import/
  migrate_profile`, token locks (`acquire_token_lock` / `hash_token`),
  `validate_profile_name`.
- **`crabrace.rs`** — `CrabraceConfig` + `CrabraceIntegration` wrapping external
  `crabrace::CrabraceClient` (provider registry at `http://localhost:8080`:
  `fetch_providers`, `get_all_model_ids`, `health_check`).
- **`health.rs`** — runtime provider health (`record_success` / `record_failure` /
  `last_working_provider` / `get_health`). Not config validation.
- **`update.rs`** — `ProviderUpdater` / `UpdateResult`.
- **`config_watcher.rs` lives in `src/utils/`** (not here): `spawn(Vec<ReloadCallback>)`
  watches config.toml/keys.toml/commands.toml via `notify`, 300ms debounce, re-runs
  `Config::load()` and fires `Arc<dyn Fn(Config)>` callbacks (keeps current config
  on reload failure).

## Adding a Config Field

1. Add the field to the relevant sub-struct in `types.rs` with `#[serde(default =
   "...")]` (or make it `Option<T>`).
2. Add a default fn + update `impl Default`.
3. Document it in `config.toml.example`.
4. Secrets: put in a separate field loaded from `keys.toml` via `merge_provider_keys`,
   typed as `SecretString`.
5. If validation matters, extend `Config::validate()`.

## Gotchas

- **`types.rs` is huge** — targeted reads only; broad greps get SIGPIPE'd.
- **Secrets**: keep in `keys.toml`, type as `SecretString` (redacts on serialize, so
  re-saving config won't leak). `expose_secret()` bypasses that — never log it.
- **Last-known-good fallback**: a broken config.toml won't crash startup; it silently
  loads stale state. Check `Config::was_recovered()`.
- **Profiles**: `resolve_profile_home()` decides the home dir; set the active profile
  *before* anything touches `stemcell_home()` (CLI does this first in `run()`).

## Tests

**New tests go in `src/tests/<area>_test.rs`** (project policy — see
`src/tests/AGENTS.md`). The inline `#[cfg(test)]` blocks in `types.rs`, `secrets.rs`,
`crabrace.rs`, `update.rs` are existing references. Integration examples in
`src/tests/`: `config_watcher_test.rs`, `profile_test.rs` (large),
`provider_config_regression_test.rs`. DB/memory data layer is in `src/db/AGENTS.md`.
