# Channels Subsystem

**Source: [`src/channels/`](../../../src/channels/)**

Multi-platform messaging integrations for LLM interaction. Every channel is **cfg-gated** behind its own feature flag — no dead code when disabled.

## Supported Channels

| Channel | Feature Flag | Crate | Capabilities |
|---------|-------------|-------|-------------|
| **Telegram** | `telegram` | teloxide 0.17 | Text, photo, voice (STT/TTS), owner DMs share TUI session, per-group sessions |
| **Discord** | `discord` | serenity 0.12 | Text, image, voice; 17 proactive actions; per-channel sessions |
| **Slack** | `slack` | slack-morphism 2 + tokio-tungstenite | Socket Mode; text, image, voice; per-channel sessions |
| **WhatsApp** | `whatsapp` | whatsapp-rust 0.6 + wacore + waproto | QR pairing; text, image, voice STT/TTS; phone allowlist |
| **Trello** | `trello` | (no external deps, pure HTTP) | 22 card/board/list actions; optional polling |
| **Voice** | (partial: `local-stt`, `local-tts`) | rwhisper (candle), rodio, OpenAI/Voicebox API | STT/TTS orchestration with fallback chains |

## Core Architecture

```
┌──────────────┐     ┌──────────────┐     ┌───────────────┐
│  Channel     │────▶│  Channel     │────▶│  AgentService │
│  Manager     │     │  Factory     │     │  (per channel)│
└──────────────┘     └──────────────┘     └───────────────┘
       │                                         │
       │ session_resolve                          │ response
       ▼                                         ▼
┌──────────────┐                         ┌──────────────┐
│  Session Init │                        │  Send/Reply  │
└──────────────┘                         └──────────────┘
```

## Key Components

- **`ChannelManager`** (`manager.rs`) — lifecycle: start, stop, restart channels; message dispatch routing.
- **`ChannelFactory`** (`factory.rs`) — creates channel-specific agent service instances from config.
- **`session_init` / `session_resolve`** — associate incoming messages with existing or new TUI/agent sessions.
- **`commands.rs`** — shared `ChannelCommands` enum for channel-specific slash commands.
- **`greeting.rs`** — generates connection greeting messages on channel startup.

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
