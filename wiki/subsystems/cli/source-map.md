# CLI & Config — Source Map

## CLI (`src/cli/`)

| File | Description |
|------|-------------|
| `mod.rs` | Module root, re-exports |
| `args.rs` | Clap `Cli` struct, `Commands` enum, all subcommand arg structs; `run()` entry point |
| `commands.rs` | Command handler functions: `cmd_status`, `cmd_doctor`, `cmd_init`, `cmd_config`, `cmd_run`, `cmd_db`, `cmd_logs`, `cmd_channel`, `cmd_memory`, `cmd_session`, `cmd_service`, `cmd_profile`, `cmd_agent_interactive`, `cmd_evolve` |
| `cron.rs` | `cmd_cron` — cron job CLI subcommand handlers |
| `crash_recovery.rs` | Crash recovery utilities |
| `daemon_health.rs` | Daemon health check reporting |
| `ui.rs` | TUI entry point (`cmd_chat`, `cmd_daemon`), UI helpers |

## Config (`src/config/`)

| File | Description |
|------|-------------|
| `mod.rs` | Module root, re-exports config types, `SecretString`, `CrabraceIntegration`, `ProviderUpdater` |
| `types.rs` | Main `Config` struct (large — defines all configuration fields: providers, database, memory, channels, logging, TUI, models, keys, etc.); `merge_provider_keys()` |
| `secrets.rs` | `SecretString` — wrapper around `zeroize::Zeroize` for API keys; auto-cleared on drop |
| `profile.rs` | `ProfileRegistry` — multi-instance profile loading/saving/touch |
| `crabrace.rs` | `CrabraceConfig`, `CrabraceIntegration` — provider registry integration |
| `health.rs` | Config validation and health check functions |
| `update.rs` | `ProviderUpdater`, `UpdateResult` — config update utilities |

## Example Configs (project root)

| File | Purpose |
|------|---------|
| `config.toml.example` | Full main config example |
| `keys.toml.example` | API keys example (secrets) |
| `commands.toml.example` | Custom commands example |
| `tools.toml.example` | Tool definitions example |
| `rtk_filters.toml.example` | RTK filters example |
| `usage_pricing.toml.example` | Usage/pricing config example |

## Related

- [Flows](flows.md)
- [Tests](tests.md)
