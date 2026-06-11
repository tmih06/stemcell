# Channels — Flows

## Surface Lifecycle (Startup + Reconcile)

The gateway owns surface lifecycle. `cli/ui.rs` builds `SurfaceDeps`, calls `registered_surfaces()` (the cfg-gated list), constructs the `Gateway`, and spawns its run loop. On startup and on every config change, `Gateway::reconcile` starts surfaces that are `Ready` and aborts listeners for surfaces that became `Inactive`.

```
cli/ui.rs startup
  │
  ▼
SurfaceDeps { agent, config_rx, per-channel state, … }
  │
  ▼
registered_surfaces(&deps)  ──▶  Vec<Arc<dyn Surface>>
  │                               (TUI always; each channel cfg-gated)
  ▼
Gateway::new(ctx, surfaces)
  │
  ▼
Gateway::reconcile(config)
  │
  ├── Surface::status(cfg) == Ready && not running ──▶ Surface::start(bus) → listener task
  └── status == Inactive   && running             ──▶ abort listener
  │
  ▼
tokio::spawn(gateway.run())   ── single inbound→agent→outbound loop
```

A channel toggled off in `build_toggles.toml` is compiled out entirely (no `pub mod`, no Cargo dependency), so it never appears in `registered_surfaces` — there is nothing to start.

## Inbound → Agent → Outbound (General)

The single pipeline every surface shares.

```
Surface receives native message
  │
  ▼
Build Inbound { surface_id, conversation_key, sender, text, routing, … }
  │
  ▼
GatewayHandle::publish_inbound(inbound)   ── bounded mpsc; drops on backpressure
  │
  ▼
Gateway run loop ── Core::process(inbound):
  │
  ├── allowlist::decide(surface_id, inbound, cfg)
  │      └── Ignore { reason } ──▶ drop (debug log only)
  │
  ├── session::resolve_for_inbound(surface_id, conversation_key)
  │      ├── TUI: conversation_key IS the session id (no DB lookup)
  │      └── Channel: suffix-stable resolve/create, honoring idle timeout
  │
  ├── AgentService turn (with surface-supplied callbacks: approval/progress/question)
  │      └── error / cancelled ──▶ drop
  │
  ▼
Build Outbound addressed back to surface_id
  │
  ▼
Surface::deliver(target, message)
  └── platform render: text chunking, image attachments, channel_messages
      recording, TTS voice reply, context-budget footer
```

The agent turn is identical regardless of origin surface — the agent only ever sees the opaque `channel` string it already records.

## Telegram Update Flow

```
Telegram polling (teloxide)
  │
  ▼
handler.rs — update received
  │
  ├──▶ /command → command handling
  │
  └──▶ Text / Photo / Voice
         │
         ├── voice note → voice::transcribe (STT)
         │
         ▼
       Build Inbound (session_hint resolved here for owner DM ↔ TUI session)
         │
         ▼
       GatewayHandle::publish_inbound
         │
         ▼
       … shared pipeline … → TelegramSurface::deliver
         └── send.rs: reply_text / reply_photo / reply_voice
```

## Voice STT/TTS Flow

```
Audio input (voice note, microphone)
  │
  ▼
voice::transcribe(bytes, voice_config)
  │
  ├── Primary STT provider
  │   ├── voicebox_stt
  │   ├── openai_stt (Groq Whisper / OpenAI-compatible)
  │   └── local_whisper (rwhisper, candle)
  │
  ├── On failure: walk stt_fallback_chain
  │   (user-configured order, e.g. ["groq", "openai_compatible", "local"])
  │
  ▼
Text  ──▶ Inbound ──▶ gateway pipeline ──▶ Response text
  │
  ▼
voice::synthesize(text, voice_config)   ── invoked in Surface::deliver
  │
  ├── voicebox_tts
  ├── openai_tts
  └── local_tts (Piper via rodio)
  │
  ▼
Audio output (voice reply)
```

## Trello Flow

```
Optional polling (or command-triggered)
  │
  ▼
client.rs::fetch_board / fetch_list / fetch_card
  │
  ▼
handler.rs → Inbound → gateway pipeline → agent interprets action
  │
  ▼
client.rs::perform_action (create/update/move card, etc.)
  │
  ▼
TrelloSurface::deliver — response formatted as message
```

## Related

- [Index](index.md)
- [Source Map](source-map.md)
- [Tests](tests.md)
