# Channels — Source Map

## Core (`src/channels/`)

| File | Type | Description |
|------|------|-------------|
| `mod.rs` | Module root | Cfg-gated submodule declarations; re-exports `ChannelFactory`, `generate_connection_greeting` |
| `factory.rs` | Factory | `ChannelFactory` — instantiate channel-specific agents from config, at startup and dynamically at runtime |
| `commands.rs` | Commands | Shared command handling across surfaces |
| `greeting.rs` | Greeting | `generate_connection_greeting()` — startup message text |
| `session_init.rs` | Init | Channel session initialization (`create_channel_session`) |
| `session_resolve.rs` | Resolve | Suffix-stable session resolution helpers for routing messages |
| `tests.rs` | Tests | Integration tests for the channel subsystem |

## Gateway (`src/channels/gateway/`)

The unified inbound→agent→outbound bus. The agent is surface-agnostic; surfaces publish onto the bus and receive deliveries back.

| File | Description |
|------|-------------|
| `gateway/mod.rs` | Module root; re-exports `Gateway`, `GatewayHandle`, `Surface`, `Inbound`/`Outbound`, `registered_surfaces`, `SurfaceDeps` |
| `gateway/surface.rs` | The `Surface` trait (`id`/`status`/`start`/`callbacks`/`deliver`); `SurfaceCallbacks`, `SurfaceStatus` |
| `gateway/bus.rs` | `Gateway` run loop + `GatewayHandle`; per-surface lifecycle via `reconcile`; the shared pipeline |
| `gateway/envelope.rs` | Normalized message types: `Inbound`, `Outbound`, `OutboundMessage`, `OutboundTarget`, `SenderRef`, `ReplyContext`, `Attachment`, `Routing` |
| `gateway/registry.rs` | `registered_surfaces()` — the single cfg-gated surface list; `SurfaceDeps` construction inputs |
| `gateway/services/mod.rs` | Shared services module root |
| `gateway/services/allowlist.rs` | Shared allowlist / respond-to policy keyed by `surface_id` |
| `gateway/services/session.rs` | Shared session resolution (`resolve_for_inbound`) keyed on `surface_id + conversation_key` |

## Surface Adapters (`src/channels/`)

Each surface implements the `Surface` trait, wiring a channel's native receive/render logic onto the bus.

| File | Feature | Description |
|------|---------|-------------|
| `tui_surface.rs` | (always) | Local terminal frontend as a peer surface |
| `telegram_surface.rs` | `telegram` | Telegram surface adapter |
| `discord_surface.rs` | `discord` | Discord surface adapter |
| `slack_surface.rs` | `slack` | Slack surface adapter |
| `whatsapp_surface.rs` | `whatsapp` | WhatsApp surface adapter |
| `trello_surface.rs` | `trello` | Trello surface adapter |

## Telegram (`src/channels/telegram/`)

**Feature:** `telegram` → **Crate:** teloxide 0.17

| File | Description |
|------|-------------|
| `mod.rs` | Module root, re-exports, `TelegramState` |
| `handler.rs` | Update handler — process incoming messages, commands, callbacks; publishes `Inbound` to the gateway |
| `agent.rs` | AgentService wrapper — conversation loop with TUI session sharing (owner DMs) |
| `send.rs` | Send helpers — text, photo, voice note replies |
| `session_resolve.rs` | Resolve sessions (owner DM → TUI session; group → per-group) |
| `follow_up_question.rs` | Follow-up question state machine |

## Discord (`src/channels/discord/`)

**Feature:** `discord` → **Crate:** serenity 0.12

| File | Description |
|------|-------------|
| `mod.rs` | Module root, re-exports, `DiscordState`, `DiscordDeliveryContext` |
| `handler.rs` | Event handler — message, voice, interaction events; publishes `Inbound` to the gateway |
| `agent.rs` | AgentService wrapper — per-channel sessions, proactive actions |
| `follow_up_question.rs` | Follow-up question handling |

## Slack (`src/channels/slack/`)

**Feature:** `slack` → **Crates:** slack-morphism 2, tokio-tungstenite

| File | Description |
|------|-------------|
| `mod.rs` | Module root, re-exports, `SlackState` |
| `handler.rs` | Socket Mode event handler; publishes `Inbound` to the gateway |
| `agent.rs` | AgentService wrapper — per-channel sessions |
| `follow_up_question.rs` | Follow-up question handling |

## WhatsApp (`src/channels/whatsapp/`)

**Feature:** `whatsapp` → **Crates:** whatsapp-rust 0.6, wacore, waproto

| File | Description |
|------|-------------|
| `mod.rs` | Module root, re-exports, `WhatsAppState` |
| `handler.rs` | Message handler — incoming messages, reply delivery; publishes `Inbound` to the gateway |
| `agent.rs` | AgentService wrapper — phone allowlist enforcement |
| `pairing.rs` | QR pairing flow — renders the pairing QR, wipes the session store on re-pair (`stemcell_home()/whatsapp`) |
| `store.rs` | Local session store for WhatsApp pairing |
| `follow_up_question.rs` | Follow-up question handling |

## Trello (`src/channels/trello/`)

**Feature:** `trello` (no external dependencies, pure HTTP API)

| File | Description |
|------|-------------|
| `mod.rs` | Module root, re-exports, `TrelloState` |
| `handler.rs` | Event/command handler |
| `agent.rs` | AgentService wrapper — card/board/list actions |
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

- [Index](index.md)
- [Tests](tests.md)
- [Flows](flows.md)
