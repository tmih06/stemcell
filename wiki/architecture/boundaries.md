# Architecture Boundaries

## Config Loading Boundary

```
config.toml + keys.toml
  → Config struct (src/config/types.rs)
  → SecretString (zeroize-on-drop, [REDACTED] debug)
  → Config hot-reload via notify watcher (src/utils/config_watcher.rs)
```

- Config files are TOML, loaded at startup with optional hot-reload
- Secrets separated into `keys.toml` with `SecretString` wrapper (zeroize-on-drop)
- Config struct is 126KB — every subsystem depends on it
- Profiles supported via `src/config/profile.rs`
- Crabrace provider registry: `src/config/crabrace.rs`

## Provider Boundary

```
Provider trait (src/brain/provider/trait.rs)
  → Factory (src/brain/provider/factory.rs)
    → Anthropic (src/brain/provider/anthropic.rs)
    → Gemini (src/brain/provider/gemini.rs)
    → Copilot (src/brain/provider/copilot.rs)
    → Qwen (src/brain/provider/qwen.rs)
    → Custom OpenAI-compatible (src/brain/provider/custom_openai_compatible.rs)
    → Claude CLI (src/brain/provider/claude_cli.rs)
    → Codex CLI (src/brain/provider/codex_cli.rs)
    → OpenCode CLI (src/brain/provider/opencode_cli.rs)
    → Placeholder (src/brain/provider/placeholder.rs)
  → Fallback chain (src/brain/provider/fallback.rs)
  → Retry wrapper (src/brain/provider/retry.rs)
  → Rate limiter (src/brain/provider/rate_limiter.rs)
```

- All providers implement the same `#[async_trait] pub trait Provider`
- Fallback chain wraps multiple providers; cascades on failure
- CLI wrappers (claude-cli, codex-cli, opencode-cli) manage their own context/tools
- Cost calculation via `calculate_cost()` (overridable per-provider) + `PricingConfig`

## Tool Boundary

```
Tool trait (src/brain/tools/trait.rs)
  → Registry (src/brain/tools/registry.rs)
    → 30+ tool implementations in src/brain/tools/
    → Browser tools (src/brain/tools/browser/)
    → Subagent tools (src/brain/tools/subagent/)
    → Dynamic tools (src/brain/tools/dynamic/)
    → Hashline editing (src/brain/tools/hashline/)
```

- All tools implement `#[async_trait] pub trait Tool`
- Registered in `registry.rs`, feature-gated via `Cargo.toml`
- Tools receive `ToolExecutionContext` (session_id, working_dir, env_vars, auto_approve, timeout, sudo_callback)

## Database Boundary

```
deadpool-sqlite connection pool
  → rusqlite (bundled SQLite)
  → Migration runner (rusqlite_migration)
  → Repository pattern (src/db/repository/)
    → session, message, channel_message, cron_job, cron_job_run, feedback_ledger,
      tool_execution, usage_ledger, plan, pending_request, file, recent_paths
```

- 24 migrations in `src/migrations/`
- Connection pool managed by deadpool-sqlite with `rt_tokio_1` feature
- Models in `src/db/models.rs`
- Retry logic in `src/db/retry.rs`

## Channel Boundary

```
ChannelFactory (src/channels/factory.rs)
  → ChannelManager (src/channels/manager.rs)
    → Telegram (teloxide) — src/channels/telegram/
    → Discord (serenity) — src/channels/discord/
    → Slack (slack-morphism Socket Mode) — src/channels/slack/
    → WhatsApp (whatsapp-rust) — src/channels/whatsapp/
    → Trello (REST API) — src/channels/trello/
    → Voice (STT/TTS) — src/channels/voice/
```

- Channels receive messages → session_resolve → AgentService → response → send back
- Each channel is feature-gated in `Cargo.toml`
- Session resolution maps platform+chat to session IDs (`src/channels/session_resolve.rs`)

## A2A Boundary

```
Axum HTTP server (src/a2a/server.rs)
  → JSON-RPC 2.0 protocol (src/a2a/types.rs)
    → Task lifecycle (Submitted, Working, Completed, Failed, Canceled, etc.)
    → AgentCard advertisement
  → Handler dispatch (src/a2a/handler/)
  → Debate (src/a2a/debate.rs)
  → Persistence (src/a2a/persistence.rs)
```

- Endpoint: `POST /rpc`
- Implements A2A spec RC v1.0 MVP subset
- Can stream responses via SSE

## FFI Boundary

- **None** — pure Rust except `libc::_exit` in `src/main.rs` (used to skip C atexit handlers on macOS ARM, avoiding llama.cpp Metal destructor crash)
- No FFI to external native libraries

## Build Boundary

```
Cargo features (Cargo.toml)
  → 90+ feature flags (per-tool, per-channel, per-capability)
  → Grouped umbrella features (tools-file-ops, tools-search, etc.)
build_toggles.toml
  → Python resolver (src/scripts/tool_features.py)
  → Produces --features list
build.rs
  → Cross-checks via OPENCRABS_EXPECTED_FEATURES env var
  → Validates resolved features match expected set
```

- Default features include: telegram, whatsapp, discord, slack, trello, browser, voice, + all tool groups
- Alternative build profiles via `./build.sh` (minimal, chatbot, telegram-agent, headless-agent)
- CI profile: thin LTO, 16 codegen units (`--profile ci`)

## Secret Boundary

- `SecretString` wrapper (via `zeroize` crate)
- `Debug` impl redacts to `[REDACTED]`
- Keys stored in separate `keys.toml` file (gitignored)
- Zeroize-on-drop for secrets in memory
- Secrets scanned via `gitleaks` in CI
