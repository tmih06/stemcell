# StemCell Wiki Index

**StemCell** v0.3.35 — a modular Rust CLI/TUI shell for LLMs. Pluggable agent with 30+ tools, multi-channel messaging (Telegram, Discord, Slack, WhatsApp, Trello), long-term memory (FTS5 + vector search), cron jobs, A2A protocol, TUI, voice I/O, and recursive self-improvement. MIT license.

## Primary Sections

| Section | Description |
|---------|-------------|
| [Agent Quickstart](agent-quickstart.md) | What agents must do before changing code |
| [Source Map](source-map.md) | File-to-responsibility mapping |
| [Change Map](change-map.md) | Common change patterns & coupled areas |
| [Entrypoints](entrypoints.md) | Binary, CLI, TUI, A2A, cron, channel entry points |
| [Flows](flows.md) | Startup, request, provider, tool, memory, compaction, channel, A2A, cron, RSI, CI flows |
| [Contracts](contracts.md) | DB schema, Provider trait, Tool trait, A2A protocol, config structure |
| [Verification](verification.md) | Make/Cargo verification commands |
| [Architecture](architecture/index.md) | System architecture summary |
| [Architecture Boundaries](architecture/boundaries.md) | Config, provider, tool, DB, channel, A2A, build, secret boundaries |
| [Brain Subsystem](subsystems/brain/index.md) | Providers, agent service, tools, RSI, tokenizer, prompt builder, mission control |
| [TUI Subsystem](subsystems/tui/index.md) | Ratatui TUI: app, render, onboarding, pane, mission control |
| [Channels Subsystem](subsystems/channels/index.md) | Telegram, Discord, Slack, WhatsApp, Trello, voice |
| [CLI & Config](subsystems/cli/index.md) | Clap CLI, config types, profiles, secrets, Crabrace |
| [Data Layer](subsystems/data/index.md) | SQLite, migrations, repositories, memory (FTS5 + vector) |
| [Infrastructure](subsystems/infra/index.md) | Build system, CI, Docker, scripts, patches |
| [Coverage Manifest](coverage-manifest.md) | Every source area with wiki coverage status |
| [Contributing Agent Rules](contributing-agent-rules.md) | Wiki update requirements for agents |

## Reading Paths by Task

| If you want to... | Read first |
|---|---|
| Add a tool | [Change Map](change-map.md) → [Source Map](source-map.md) → `src/brain/tools/` |
| Add a provider | [Change Map](change-map.md) → [Contracts](contracts.md) → `src/brain/provider/` |
| Add a channel | [Change Map](change-map.md) → [Source Map](source-map.md) → `src/channels/` |
| Fix the agent loop | [Change Map](change-map.md) → [Flows](flows.md) → `src/brain/agent/service/` |
| Understand startup | [Flows](flows.md) → [Entrypoints](entrypoints.md) |
| Tweak config | [Contracts](contracts.md) → `src/config/types.rs` |
| Debug a channel issue | [Flows](flows.md) → [Source Map](source-map.md) → `src/channels/` |
| Run tests | [Verification](verification.md) |
| Understand memory | [Flows](flows.md) → `src/memory/` |
| Work on RSI | [Flows](flows.md) → `src/brain/rsi*.rs` + `src/brain/mission_control/` |

## Important Unknowns

- **Crabrace** (`src/config/crabrace.rs`): Provider registry. Exact API contract not fully documented in wiki.
- **Phantom language** (`src/brain/agent/service/phantom_lang/`): Internal DSL for agent behavior. Needs subsystem doc.
- **Hashline editing** (`src/brain/tools/hashline/`): Precise line-level file editing. Semantics not yet captured.
- **Dynamic tools** (`src/brain/tools/dynamic/`): User-defined tools from `tools.toml`. Runtime loading mechanics not documented.
- **Config hot-reload** (`src/utils/config_watcher.rs`): Notify-based watcher. Boundaries and triggers not specified.
- **Docker**: Dockerfile presence and usage not verified.

## Exclusions

- Third-party patches in `src/patches/` (wacore-binary patches for WhatsApp)
- GitHub issue/PR templates (`.github/`)
- `build_toggles.toml` — build profile definitions (covered in [Infrastructure](subsystems/infra/index.md))
- `src/docker/` — Docker-related files
- Generated files will be listed in [Source Map](source-map.md)

## Caveats

- Wiki links to subsystems (`subsystems/brain/index.md` etc.) are stubs — agents must read source for full detail.
- Configuration structure is large (`src/config/types.rs` is 126KB). Consult [Contracts](contracts.md) for the skeleton.
- Test files (228 files, ~2900 tests) are in `src/tests/`. Not every test file is individually mapped.
- Feature flags control compilation aggressively — always build with `--all-features` for CI or `build_toggles.toml` for dev.
