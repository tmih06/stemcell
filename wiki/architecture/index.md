# Architecture

StemCell is a single Rust binary (`src/main.rs` + `src/lib.rs`, 1 crate) with compile-time feature toggles.

## Core Pattern

```
AppState
  → CLI / TUI / Channel entry
    → AgentService
      → LLM Provider (via Provider trait)
        → Tool execution (via Tool trait)
          → Persistence (SQLite via deadpool-sqlite)
```

## Modularity

| Mechanism | Interface | Implementations |
|-----------|-----------|-----------------|
| **Provider trait** | `src/brain/provider/trait.rs` | Anthropic, Gemini, Copilot, Qwen, OpenAI-compat, CLI wrappers |
| **Tool trait** | `src/brain/tools/trait.rs` | 30+ tools (bash, file I/O, search, browser, subagents, etc.) |
| **Channel pattern** | `src/channels/factory.rs` + `manager.rs` | Telegram, Discord, Slack, WhatsApp, Trello, Voice |
| **Build-time toggles** | `Cargo.toml` features + `build_toggles.toml` | Per-tool, per-channel, per-capability feature flags |

## Key Properties

- **Single binary** — everything statically linked, no plugin system
- **Feature-gated compilation** — `Cargo.toml` has 90+ feature flags, `build_toggles.toml` drives build profiles, `build.rs` cross-checks expected features via `STEMCELL_EXPECTED_FEATURES`
- **Local-first** — SQLite storage, no cloud dependency
- **Session-oriented** — persistent chat sessions with token/cost tracking
- **Concurrent channels** — multi-platform messaging through shared agent sessions

## Module Dependencies

```
src/main.rs
  → src/lib.rs (declares all modules)
    → src/cli/        CLI argument parsing + dispatch
    → src/config/     TOML config, secrets, profiles
    → src/db/         SQLite pool + repository layer
    → src/brain/      Core AI (providers + agent + tools)
    → src/channels/   Messaging platform integrations
    → src/tui/        Terminal UI
    → src/a2a/        Agent-to-Agent protocol
    → src/cron/       Scheduled tasks
    → src/memory/     FTS5 + vector search
    → src/services/   Business logic
    → src/logging/    Tracing logger
    → src/startup/    Startup jobs
    → src/rtk/        Rust Token Killer
    → src/usage/      Usage analytics
    → src/utils/      Shared utilities
```

## Subsystems

See [subsystem pages](../subsystems/brain/index.md), [TUI](../subsystems/tui/index.md), [Channels](../subsystems/channels/index.md), [CLI & Config](../subsystems/cli/index.md), [Data](../subsystems/data/index.md), [Infrastructure](../subsystems/infra/index.md) for detailed coverage.
