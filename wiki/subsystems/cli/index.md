# CLI & Config Subsystem

**CLI Source: [`src/cli/`](../../../src/cli/)**

**Config Source: [`src/config/`](../../../src/config/)**

**Example Configs:** Root directory (`config.toml.example`, `keys.toml.example`, etc.)

## CLI (`src/cli/`)

Clap v4.5-based argument parsing with command dispatch. The binary entry point parses args, loads config, then dispatches to the matching handler.

### Subcommands

| Command | Description |
|---------|-------------|
| `chat` | Interactive TUI session (default) |
| `run` | Non-interactive single prompt |
| `agent` | Headless agent mode (stdin/stdout) |
| `status` | System status overview |
| `doctor` | Full diagnostics |
| `config` | View configuration |
| `memory` | Brain memory file operations |
| `session` | Session list/details |
| `db` | Database init/stats/clear |
| `cron` | Cron job management |
| `logs` | Log viewing/cleanup |
| `service` | OS service (launchd/systemd) management |
| `daemon` | Headless daemon mode (channel bots only) |
| `completions` | Shell completions generation |
| `profile` | Multi-instance profile management |
| `init` | First-time configuration setup |
| `onboard` | Onboarding wizard |
| `channel` | Channel list/health |
| `evolve` | Self-update (check/install) |
| `version` | Print version and exit |

### Dispatch Flow

```
main()
  │
  ▼
Cli::parse()              (args.rs)
  │
  ├── profile set
  ├── load_config()
  └── match command:
       ├── Chat   → ui::cmd_chat()
       ├── Run    → commands::cmd_run()
       ├── Agent  → commands::cmd_agent_interactive()
       ├── Daemon → ui::cmd_daemon()
       ├── Cron   → cron::cmd_cron()
       ├── ...    → commands::cmd_*()
       └── Completions → clap_complete::generate()
```

## Config (`src/config/`)

TOML-based configuration system with secret separation and hot-reload.

### Config Files

| File | Purpose |
|------|---------|
| `config.toml` | Main config (providers, database, memory, channels, logging, TUI, models) |
| `keys.toml` | API keys only (zeroize-on-drop via `SecretString`) |
| `commands.toml.example` | Example custom commands |
| `tools.toml.example` | Example tool definitions |
| `rtk_filters.toml.example` | Example RTK filters |
| `usage_pricing.toml.example` | Example usage/pricing config |

### Profile System

Multiple named instances, each with isolated config, database, and memory. Managed via `profile` subcommand.

### Key Features

- **Secret isolation**: API keys in separate `keys.toml`, wrapped in `SecretString` (zeroize-on-drop, `src/config/secrets.rs`)
- **Hot-reload**: Config file watching via `notify` crate
- **Crabrace**: Provider registry integration (`src/config/crabrace.rs`)
- **Validation**: Health checks (`src/config/health.rs`)

## Related

- [Source Map](source-map.md)
- [Flows](flows.md)
- [Tests](tests.md)
