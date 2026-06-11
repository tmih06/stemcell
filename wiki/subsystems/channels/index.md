# Channels Subsystem

**Source: [`src/channels/`](../../../src/channels/)**

Multi-platform messaging integrations for LLM interaction. Every channel is **cfg-gated** behind its own feature flag — a channel toggled off in `build_toggles.toml` contributes no source, no symbols, and no binary footprint.

## The Gateway Model

Channels are **not** agent tools. They are **remote surfaces**: a message arriving from Telegram (or Discord, Slack, WhatsApp, or the TUI) enters the agent exactly like a TUI prompt, the agent replies with its normal loop knowing nothing about channels, and the gateway routes the response back out the surface it came from. The TUI and every channel are peer surfaces on one async bus.

This replaces the legacy `ChannelManager` / per-channel agent-tool design. The agent core never learns which surface a message came from beyond the opaque `channel` string it already records — there are no channel tools and no per-surface branches in the agent loop.

```
┌───────────┐  Inbound   ┌──────────────────────────────┐  Outbound  ┌───────────┐
│  Surface  │───────────▶│           Gateway            │───────────▶│  Surface  │
│ (Telegram,│  publish   │  allowlist → session resolve │  deliver   │  (same    │
│  Discord, │            │  → agent turn → post-process │            │  surface) │
│  TUI, …)  │            └──────────────────────────────┘            └───────────┘
                                       │
                                       ▼
                              ┌──────────────┐
                              │ AgentService │  (surface-agnostic)
                              └──────────────┘
```

## Supported Channels

| Channel | Feature Flag | Crate | Capabilities |
|---------|-------------|-------|-------------|
| **Telegram** | `telegram` | teloxide 0.17 | Text, photo, voice (STT/TTS), owner DMs share TUI session, per-group sessions |
| **Discord** | `discord` | serenity 0.12 | Text, image, voice; proactive actions; per-channel sessions |
| **Slack** | `slack` | slack-morphism 2 + tokio-tungstenite | Socket Mode; text, image, voice; per-channel sessions |
| **WhatsApp** | `whatsapp` | whatsapp-rust 0.6 + wacore + waproto | QR pairing; text, image, voice STT/TTS; phone allowlist |
| **Trello** | `trello` | (no external deps, pure HTTP) | Card/board/list actions; optional polling |
| **TUI** | (always present) | — | The local terminal frontend, a peer surface on the bus |
| **Voice** | (partial: `local-stt`, `local-tts`) | rwhisper (candle), rodio, OpenAI/Voicebox API | STT/TTS orchestration with fallback chains |

## The Gateway (`src/channels/gateway/`)

The unified bus lives in [`gateway/`](../../../src/channels/gateway/). Its module root [`gateway/mod.rs`](../../../src/channels/gateway/) re-exports the public surface.

| File | Role |
|------|------|
| `gateway/surface.rs` | The `Surface` trait every frontend implements: `id()`, `status()`, `start()`, `callbacks()`, `deliver()`. Object-safe so the gateway holds `Vec<Arc<dyn Surface>>`. Also defines `SurfaceCallbacks` and `SurfaceStatus`. |
| `gateway/bus.rs` | The `Gateway` run loop + `GatewayHandle` producer. Owns the single inbound→agent→outbound pipeline and per-surface lifecycle (`reconcile` starts/stops listeners on config change). |
| `gateway/envelope.rs` | Normalized `Inbound` / `Outbound` message types (plus `SenderRef`, `ReplyContext`, `Attachment`, `Routing`). The vocabulary the agent never sees. |
| `gateway/registry.rs` | The **single** cfg-gated list of compiled-in surfaces (`registered_surfaces`) and the `SurfaceDeps` they construct from. Adding a channel = one `#[cfg]` push here. |
| `gateway/services/allowlist.rs` | Shared allowlist / respond-to policy, keyed by `surface_id`. Surfaces supply platform facts via `Inbound::routing`; this applies the one shared rule. |
| `gateway/services/session.rs` | Shared session resolution (`resolve_for_inbound`) keyed on `surface_id + conversation_key`, re-exporting the suffix-stable resolver. |

## Surfaces

Each channel exposes a thin surface adapter that implements the `Surface` trait — it listens for native messages, publishes `Inbound` envelopes, and delivers responses. The per-channel `handler.rs` / `agent.rs` modules still hold the platform-specific receive and rendering logic; the surface wires them onto the bus.

| Surface | Source |
|---------|--------|
| TUI | `tui_surface.rs` |
| Telegram | `telegram_surface.rs` |
| Discord | `discord_surface.rs` |
| Slack | `slack_surface.rs` |
| WhatsApp | `whatsapp_surface.rs` |
| Trello | `trello_surface.rs` |

## Shared Components

- **`ChannelFactory`** (`factory.rs`) — creates channel-specific agent service instances from config; used at startup and by dynamic runtime connection.
- **`session_init.rs` / `session_resolve.rs`** — associate incoming messages with existing or new TUI/agent sessions (the lower-level helpers the gateway's session service re-exports).
- **`commands.rs`** — shared command handling across surfaces.
- **`greeting.rs`** — `generate_connection_greeting()` produces connection greeting text on surface startup.

## Voice Subsystem

**Source: [`src/channels/voice/`](../../../src/channels/voice/)**

| Service | Module | Backend |
|---------|--------|---------|
| Local STT | `local_whisper.rs` | rwhisper (candle, AVX2-gated on x86_64) |
| Local TTS | `local_tts.rs` | rodio + Piper (python3) |
| OpenAI STT | `openai_stt.rs` | OpenAI-compatible API (Groq Whisper) |
| OpenAI TTS | `openai_tts.rs` | OpenAI TTS API |
| Voicebox STT | `voicebox_stt.rs` | Voicebox API |
| Voicebox TTS | `voicebox_tts.rs` | Voicebox API |
| Orchestration | `service.rs` | Fallback-chain STT/TTS dispatch |

The `service.rs` orchestrator resolves a primary STT provider and walks a user-configured `stt_fallback_chain` on failure.

## Related

- [Source Map](source-map.md)
- [Flows](flows.md)
- [Tests](tests.md)
