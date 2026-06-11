# src/channels/ — Messaging Channels

Multi-platform messaging integrations (Telegram, Discord, Slack, WhatsApp, Trello)
plus the voice subsystem. **Every channel is `#[cfg]`-gated behind its own feature
flag** — disabled channels compile to zero code.

## Structure

```
src/channels/
  mod.rs              # cfg-gated submodules; re-exports ChannelFactory, ChannelManager
  manager.rs          # ChannelManager — lifecycle (spawn/stop/reconcile on config reload)
  factory.rs          # ChannelFactory — builds per-channel AgentService from shared state
  commands.rs         # ChannelCommands enum + provider_section() (shared slash-command logic)
  greeting.rs         # generate_connection_greeting()
  session_init.rs     # session initialization
  session_resolve.rs  # shared resolver: channel+chat → session via stable [chat:<id>] suffix
  tests.rs            # #[cfg(test)] unit tests
  discord/ slack/ telegram/ trello/ whatsapp/   # per-channel impls
  voice/              # STT/TTS orchestration (used BY channels, not a channel)
```

Per-channel module layout (consistent convention):
- `mod.rs` — module root + a `<Name>State` struct (bot handle, session↔chat maps,
  pending approvals, pending follow-up questions, cancel tokens). Re-exports `<Name>Agent`.
- `handler.rs` — platform event/update handler (incoming messages, commands, callbacks).
- `agent.rs` — `<Name>Agent` wrapping an `AgentService`; owns `start()` which spawns
  the listener loop and returns a `JoinHandle<()>`.
- `follow_up_question.rs` — follow-up question state machine (all except Trello).
- Extras: telegram has `send.rs`, `session_resolve.rs`, `rolling_status_quips.rs`;
  trello has `client.rs` (HTTP) + `models.rs`; whatsapp has `store.rs` (pairing).

## ChannelManager (`manager.rs`)

Owns lifecycle. `reconcile(&self, config: &Config)` runs on config hot-reload:
compares running channels (`handles: Mutex<HashMap<String, JoinHandle<()>>>`)
against config and spawns/stops each via a `reconcile_<channel>` method (one per
channel, each `#[cfg(feature=...)]`-gated). Per channel:

- compute `should_run = cfg.enabled && has_valid_token(s)` (per-channel token
  validation — Telegram `id:secret`, Slack `xoxb-`/`xapp-`, Discord len>50, Trello
  key+token+board_ids).
- `acquire_token_lock(channel, hash)` via `crate::config::profile` to stop two
  processes sharing one bot token.
- build `<Name>Agent::new(factory.create_agent_service().await,
  factory.service_context(), factory.shared_session_id(), <name>_state.clone(),
  factory.config_rx(), ChannelMessageRepository::new(db_pool))`, then
  `handles.insert(name, agent.start(token...))`. Stop = `handles.remove(name).abort()`.

The struct, its fields, and `new()` args are all cfg-gated; disabling every channel
compiles ChannelManager to an empty shell.

## Adding a Channel

Channels do **not** implement a shared Rust trait. The "interface" is a convention:
a `<Name>Agent` with `new(...)` + `start(...) -> JoinHandle<()>`, plus a
`<Name>State`. `ChannelFactory` + `ChannelManager` wire them by hand.

1. `Cargo.toml` `[features]` — add `mychannel = ["<sdk-crate>"]`, add the optional
   dep, add to `default = [...]` if on by default.
2. `src/channels/mod.rs` — `#[cfg(feature = "mychannel")] pub mod mychannel;`.
3. `src/channels/mychannel/` — create `mod.rs` (+ `MyChannelState`, re-export
   `MyChannelAgent`), `handler.rs`, `agent.rs` (`pub fn start(self, ...) ->
   JoinHandle<()>`), optional `follow_up_question.rs`, `client.rs`/`send.rs`.
4. `src/channels/manager.rs` — add cfg-gated `<name>_state` field + `new()` arg,
   add `mychannel` to **every** `#[cfg(any(feature = ...))]` umbrella list (~8
   places: fields, new() args, struct init, reconcile), add `reconcile_mychannel()`
   and call it in `reconcile()`.
5. `src/channels/session_resolve.rs` — reuse `resolve_or_create_channel_session()`
   with `chat_id_suffix()` (Discord/Slack/WhatsApp pattern). Only write a bespoke
   resolver if you need in-memory chat→session binding (Telegram does).
6. `src/config/types.rs` — add a `channels.mychannel` config struct.
7. `src/brain/tools/` — optionally add `mychannel_connect.rs` + `mychannel_send.rs`,
   gate behind `tool-mychannel-connect`/`tool-mychannel-send` under
   `tools-channel-integrations`, register in `modules.rs` (see `src/brain/AGENTS.md`).
8. Tests + update wiki `subsystems/channels/`.

## Feature Flags (`Cargo.toml`)

```
telegram = ["teloxide"]                       discord = ["serenity"]
slack    = ["slack-morphism","tokio-tungstenite","rustls"]
whatsapp = ["whatsapp-rust", "wacore", "wacore-binary", "waproto", "dep:qrcode", ...]
trello   = []   # pure HTTP, no SDK
local-stt / local-tts = [...]                 # voice (whisper / piper)
```

## Shared Patterns

- **Factory** (`factory.rs`): `ChannelFactory` holds shared provider,
  `ServiceContext`, `shared_brain (RwLock<String>)`, `tool_registry (OnceLock`, set
  lazily to break a circular dep), `shared_session_id`, and `config_rx:
  watch::Receiver<Config>` (always reads latest config — live voice/TTS keys).
  `create_agent_service()` builds a fresh `AgentService` per channel.
- **State structs** carry async `Mutex`-guarded maps: session↔chat bidirectional
  maps, `pending_approvals` (oneshot), `pending_questions` (Telegram encodes only
  the option index in callback data — 64-byte limit), per-session `cancel_tokens`
  (a new call cancels the prior in-flight one; `/stop`), photo-album debounce (3s).
- **Session resolution** (`session_resolve.rs`): embeds a stable `[chat:<id>]`
  suffix at creation, looks up by suffix, one-shot migrates legacy title-based rows.
- **Voice** (`voice/service.rs`): `transcribe(bytes, &VoiceConfig)` /
  `synthesize(...)` orchestrate a primary provider + a user `stt_fallback_chain`
  (e.g. `["groq","openai_compatible","local"]`). Providers: `voicebox_stt/tts`,
  `openai_stt/tts`, `local_whisper`, `local_tts`. Returns first success; composite
  error on full failure. Called by channels, never standalone.

## Gotchas

- **WhatsApp wacore-binary patch** (`src/patches/wacore-binary/`): a vendored fork
  fixing portable_simd breakage on recent nightlies. **NOT currently wired via
  `[patch.crates-io]`** — Cargo.toml references upstream `wacore-binary = "0.6.0"`.
  If WhatsApp fails to build on a recent nightly, a `[patch.crates-io]` entry
  pointing at the vendored path is likely needed. Verify before relying on it.
- **Manager umbrella cfg lists** repeat the same `#[cfg(any(...))]` in ~8 places —
  miss one when adding a channel and you get compile errors / dead fields.
- **Slack rustls coupling**: keep the explicit `slack = [..., "rustls"]` set;
  slack-morphism's `rustls-native-certs` maps to the wrong tokio-tungstenite (see
  Cargo.toml comment).
- **Approval**: setting `auto_approve_tools` silently disables a channel's per-call
  `override_approval_callback`. Channels with their own approval flow must pass
  `override_approval_callback` per call and NOT set `auto_approve_tools`.
- Voice STT fallback re-clones multi-MB audio per attempt.

## Tests

**New tests go in `src/tests/<area>_test.rs`** (project policy — see
`src/tests/AGENTS.md`), not new inline blocks. `src/channels/tests.rs`
(`#[cfg(test)]`, currently `commands::provider_section`) and per-channel inline
feature-gated tests are existing references. Trello uses **mockito** for HTTP. Run:
`cargo test --features telegram,discord,slack,whatsapp,trello`; voice:
`cargo test --features local-stt,local-tts ... voice`. CI uses `--all-features`.
Wiki: `wiki/subsystems/channels/tests.md`.
