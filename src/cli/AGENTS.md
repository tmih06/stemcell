# src/cli/ + Infra — Boot, CLI, A2A, Cron, Build

Covers the binary entrypoint, CLI, and the infra subsystems (`src/a2a/`,
`src/cron/`, `src/startup/`, `src/services/`, `src/usage/`, `src/rtk/`,
`src/logging/`, `src/utils/`) plus the build system.

## Boot (`src/main.rs`)

`#[tokio::main]` async `main()`: (slack) install rustls ring provider →
`cli::Cli::parse()` for the early `--debug` flag → `logging::init_logging` (holds a
`WorkerGuard`; honors `DEBUG_LOGS_LOCATION`) → clean old logs + orphaned channel
temp files → `cli::run().await`. Exits via `unsafe { libc::_exit(code) }` to skip a
llama.cpp Metal destructor crash on macOS ARM and force-kill lingering tokio tasks.

## CLI (`src/cli/`, clap v4 derive)

- `mod.rs` declares `args`, `commands`, `crash_recovery`, `cron`, `daemon_health`,
  `ui`. `args.rs` (~13KB): `Cli` struct (global `--debug`, `--config`, `--profile`,
  optional `command`) + `run()`.
- `run()` (`args.rs`): sets active profile **before** anything touches
  `stemcell_home()`, loads config via `commands::load_config()`, auto-generates
  `config.toml` if API keys are in env but no file exists, then matches the subcommand.
- Subcommands: nested enums `DbCommands`, `LogCommands`, `CronCommands`,
  `ChannelCommands`, `MemoryCommands`, `SessionCommands`, `ProfileCommands`,
  `ServiceCommands` + `OutputFormat` (Text/Json/Markdown). Dispatch: `ui::cmd_chat`
  (Chat/Onboard default), `ui::cmd_daemon`, `cron::cmd_cron`, rest in
  `commands::cmd_*`. Handlers in `commands.rs` (59KB) and `ui.rs` (55KB — TUI/daemon
  launch). `Agent { message }`: `--message` → `cmd_run` (single-shot), else
  `cmd_agent_interactive`.

## Build System (three-layer feature model)

- Cargo features: fine-grained `tool-*` flags; `tools-*` are compatibility-alias
  groups. Source gates with `#[cfg(feature = "...")]`. `default` = all channels +
  all tool groups.
- **`build_toggles.toml`** (repo root) is the human dev switchboard — boolean pack
  toggles grouped `[capabilities]`/`[channels]`/`[file]`/… NOT raw cargo features.
- **Resolution**: `make build` runs `src/scripts/tool_features.py build_toggles.toml`
  → emits a `--features` set → `cargo build --no-default-features --features "$SET"`.
  The toggle→feature map, `IMPLIES`, and `ALIAS_FROM_PACKS` are **duplicated** in the
  Python script AND `build.rs` — keep in sync.
- **`build.rs`** does NOT codegen. It validates `build_toggles.toml` keys (panics on
  stale/typo toggles) and, if `STEMCELL_EXPECTED_FEATURES` is set (the Makefile sets
  it), recomputes the active feature set and **panics on mismatch** — this enforces
  "use the Makefile, not raw cargo".
- **`build-profiles.toml` + `build.sh`** — simpler bash-parsed named profiles
  (`full`, `minimal`, `chatbot`, `headless-agent`, …). `./build.sh <profile>`.
- **Why `--all-features` matters**: tools are individually gated, so a toggle build
  compiles only a subset. Only `--all-features` (used by `make check/lint/test/doc`
  + CI) compiles every `cfg` branch.
- Cargo profiles: `dev` (opt 0), `release` (fat LTO), `release-small` (opt z), `ci`
  (thin LTO, 16 codegen-units — fast CI). MSRV 1.91, edition 2024.

See the root `AGENTS.md` for the full Makefile/verify command list.

## Infra Subsystems

- **`a2a/`** — Agent-to-Agent (A2A Protocol RC v1.0): axum HTTP + JSON-RPC 2.0.
  `server.rs`: `A2aState`, routes `GET /.well-known/agent.json`, `GET /a2a/health`
  (public), `POST /a2a/v1` (optional bearer). `message/stream` returns SSE; other
  methods via `handler::dispatch`. `start_server` restores in-flight tasks from DB
  on boot; no-op if `config.enabled == false`. `handler/` (send/stream/tasks),
  `types.rs`, `agent_card.rs`, `debate.rs` (multi-agent "Bee Colony"),
  `persistence.rs`. Gated at the tool layer behind `tool-a2a-send`/`brain`.
- **`cron/`** — `scheduler.rs`: `CronScheduler` background task polls the
  `cron_jobs` table every 60s, runs jobs in an isolated `"Cron"` session, delivers
  results via `ChannelFactory`. CLI side `src/cli/cron.rs` (`cron add/list/remove/
  enable/disable/test`).
- **`startup/`** — startup jobs run on every TUI boot, parallel, non-fatal.
  `default_jobs()`: `CheckConfigJob`, `CheckEnvsJob`, `RsiStatusJob`,
  `RsiProposalsJob`, `RsiDigestJob`, `FetchModelsJob`. Emits one collapsible
  `TuiEvent::StartupInfo` and flips a `ready_tx` watch channel.
- **`services/`** — business layer over the pool: `ServiceContext { pool }` +
  `ServiceManager` (Session/Message/File/Plan services). Threaded into A2A, cron.
- **`usage/`** — full-screen TUI analytics: `data.rs` (DB queries), `cards.rs` (6
  cards), `dashboard.rs`, `categorizer.rs`, `pricing.rs`.
- **`rtk/`** — wraps the external `rtk` CLI to compress bash output (feature `rtk`;
  no-op stubs otherwise). `rewrite.rs` prepends `rtk` for allowlisted commands;
  `tracker.rs` records savings. See root `AGENTS.md` for usage.
- **`logging/`** — `tracing` subscriber (EnvFilter, local-time, optional JSON, file
  output via `tracing-appender`, rotation/cleanup). Default dir `stemcell_home()/logs`.
- **`utils/`** — shared helpers: `sanitize.rs` (output redaction), `file_extract.rs`,
  `retry.rs`, `config_watcher.rs` (hot-reload), `pdf_vision.rs`, `approval.rs`,
  `git_branch.rs`, `install.rs`, `tool_context.rs`.

## Gotchas

- Never hand-roll `cargo build --features` — use `make build` (resolver +
  `STEMCELL_EXPECTED_FEATURES` enforcement). Editing tools/features means updating
  `Cargo.toml` AND both maps (`build.rs` + `tool_features.py`) AND `build_toggles.toml`.
- A2A `start_server` is a no-op when disabled — check `config.enabled` if it "won't start".
- Cron jobs never run in the user's TUI session — always the isolated `"Cron"` session.

## Tests

`src/tests/`: `cli_test.rs`, `cli_arg_too_long_test.rs`, `cli_supported_models_test.rs`,
`cron_test.rs`, `rtk_rewrite_test.rs`, `rtk_sysadmin_supported_test.rs`,
`rtk_tracker_test.rs`, `usage_*_test.rs`, `mission_control_*_service_test.rs`. Inline
`#[cfg(test)]` in `a2a/server.rs`, `services/context.rs`; A2A has `test_helpers.rs`.
