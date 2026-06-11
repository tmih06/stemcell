# Channels — Source Map

## Core (`src/channels/`)

| File | Type | Description |
|------|------|-------------|
| `mod.rs` | Module root | Re-exports `ChannelFactory`, `ChannelManager`, `generate_connection_greeting`; cfg-gated submodules |
| `manager.rs` | Lifecycle | `ChannelManager` — start/stop/restart channels, route incoming messages |
| `factory.rs` | Factory | `ChannelFactory` — instantiate channel-specific agents from config |
| `commands.rs` | Commands | `ChannelCommands` enum shared across channels |
| `greeting.rs` | Greeting | `generate_connection_greeting()` — startup message text |
| `session_init.rs` | Init | Channel session initialization logic |
| `session_resolve.rs` | Resolve | Session resolution helpers for routing messages |
| `tests.rs` | Tests | Integration tests for channel subsystem |

## Telegram (`src/channels/telegram/`)

**Feature:** `telegram` → **Crate:** teloxide 0.17

| File | Description |
|------|-------------|
| `mod.rs` | Module root, re-exports |
| `handler.rs` | Update handler — process incoming messages, commands, callbacks |
| `agent.rs` | AgentService wrapper — conversation loop with TUI session sharing (owner DMs) |
| `send.rs` | Send helpers — text, photo, voice note replies |
| `session_resolve.rs` | Resolve sessions (owner DM → TUI session; group → per-group) |
| `follow_up_question.rs` | Follow-up question state machine |
| `rolling_status_quips.rs` | Rotating status messages for long-running operations |

## Discord (`src/channels/discord/`)

**Feature:** `discord` → **Crate:** serenity 0.12

| File | Description |
|------|-------------|
| `mod.rs` | Module root, re-exports |
| `handler.rs` | Event handler — message, voice, interaction events |
| `agent.rs` | AgentService wrapper — per-channel sessions, 17 proactive actions |
| `follow_up_question.rs` | Follow-up question handling |

## Slack (`src/channels/slack/`)

**Feature:** `slack` → **Crates:** slack-morphism 2, tokio-tungstenite

| File | Description |
|------|-------------|
| `mod.rs` | Module root, re-exports |
| `handler.rs` | Socket Mode event handler |
| `agent.rs` | AgentService wrapper — per-channel sessions |
| `follow_up_question.rs` | Follow-up question handling |

## WhatsApp (`src/channels/whatsapp/`)

**Feature:** `whatsapp` → **Crates:** whatsapp-rust 0.6, wacore, waproto

| File | Description |
|------|-------------|
| `mod.rs` | Module root, re-exports |
| `handler.rs` | Message handler — QR pairing, incoming messages |
| `agent.rs` | AgentService wrapper — phone allowlist enforcement |
| `store.rs` | Local session store for WhatsApp pairing |
| `follow_up_question.rs` | Follow-up question handling |

## Trello (`src/channels/trello/`)

**Feature:** `trello` (no external dependencies, pure HTTP API)

| File | Description |
|------|-------------|
| `mod.rs` | Module root, re-exports |
| `handler.rs` | Event/command handler |
| `agent.rs` | AgentService wrapper — 22 card/board/list actions |
| `client.rs` | HTTP client wrapping Trello REST API |
| `models.rs` | Trello data models (card, board, list, etc.) |

## Voice (`src/channels/voice/`)

**Features:** (partial: `local-stt`, `local-tts`)

| File | Backend | Description |
|------|---------|-------------|
| `mod.rs` | — | Module root, re-exports `transcribe`, `synthesize`, availability probes |
| `service.rs` | — | Orchestration: primary + fallback-chain STT/TTS dispatch |
| `local_whisper.rs` | rwhisper (candle) | Local STT via whisper.cpp bindings (AVX2-gated on x86_64) |
| `local_tts.rs` | rodio + Piper (python3) | Local TTS via Piper |
| `openai_stt.rs` | OpenAI API | STT via OpenAI-compatible / Groq Whisper API |
| `openai_tts.rs` | OpenAI API | TTS via OpenAI TTS API |
| `voicebox_stt.rs` | Voicebox API | STT via Voicebox API |
| `voicebox_tts.rs` | Voicebox API | TTS via Voicebox API |

## Related

- [Tests](tests.md)
- [Flows](flows.md)
