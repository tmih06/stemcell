# Entrypoints

## Binary Entry

| Entry | File | Description |
|-------|------|-------------|
| `main()` | `src/main.rs` | Parses CLI args, inits logging, dispatches to `cli::run()` |

## CLI Subcommands

All dispatched from `src/cli/commands.rs` (59KB). CLI parsing in `src/cli/args.rs` (13KB).

| Subcommand | Description |
|------------|-------------|
| `chat` | Interactive REPL-style chat |
| `run` | Non-interactive single-prompt execution |
| `agent` | Agent mode (session-based) |
| `status` | System status overview |
| `doctor` | Diagnostics check |
| `config` | Config management subcommands |
| `memory` | Memory management |
| `session` | Session management |
| `db` | Database operations |
| `cron` | Cron job management |
| `logs` | Log viewing |
| `service` | Service mode |
| `daemon` | Daemon mode |
| `completions` | Shell completion generation (`src/clap` + `clap_complete`) |
| `profile` | Config profile management |
| `init` | Initial setup |
| `onboard` | Onboarding wizard |
| `channel` | Channel management |
| `version` | Version info |

## TUI

| Entry | File | Description |
|-------|------|-------------|
| TUI runner | `src/tui/runner.rs` | Crossterm event loop, Ratatui rendering |

## A2A HTTP Server

| Entry | File | Description |
|-------|------|-------------|
| HTTP server | `src/a2a/server.rs` | Axum HTTP server, JSON-RPC 2.0 protocol |

## Cron Scheduler

| Entry | File | Description |
|-------|------|-------------|
| Scheduler | `src/cron/scheduler.rs` | Polls DB for due cron jobs, executes in active sessions |

## Channel Connections

| Entry | File |
|-------|------|
| Telegram | `src/channels/telegram/` (teloxide bot) |
| Discord | `src/channels/discord/` (serenity gateway) |
| Slack | `src/channels/slack/` (slack-morphism Socket Mode) |
| WhatsApp | `src/channels/whatsapp/` (whatsapp-rust Web) |
| Trello | `src/channels/trello/` (Trello REST API) |
| Voice | `src/channels/voice/` (STT via rwhisper, TTS via opusic-sys) |

## Tests

| Entry | Description |
|-------|-------------|
| `cargo test` | Runs ~2900 tests across 228 files in `src/tests/` |
| `src/tests/mod.rs` | Test module root (if exists) |
| `src/benches/database.rs` | Database benchmarks |
| `src/benches/memory.rs` | Memory benchmarks |
