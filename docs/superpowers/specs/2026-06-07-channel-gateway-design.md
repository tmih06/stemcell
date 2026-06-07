# Channel Gateway Refactor — Design

**Date:** 2026-06-07
**Status:** Approved for planning

## Problem

The channel system (Telegram, WhatsApp, Discord, Slack, Trello) has no common
abstraction. Every integration point special-cases all five platforms:

- `ChannelManager::reconcile` — five near-identical `reconcile_x` methods, each
  buried under `#[cfg(any(telegram, whatsapp, ...))]` walls.
- `ChannelManager` / `ChannelFactory` / `cli/ui.rs` — five `#[cfg]`-gated state
  fields and clone-args.
- `cli/ui.rs:798` resume-recovery — a `match channel.as_str()` hand-writing the
  outbound send for each platform.
- Five `*_send` tools + five `*_connect` tools, each repeating credential / send
  logic, all appended to the agent's tool context.
- Each `XAgent` has a bespoke `new()` (Telegram: 1 token; Slack: 2 tokens;
  Trello: key+token; WhatsApp: none) and bespoke `start()`.

`ChannelsConfig` already declares `signal`, `google_chat`, `imessage` with no
implementations — the codebase wants "add a channel" to be trivial, but every
new one currently means editing ~8 call sites.

## Core reframe: channels are surfaces, not tools

The defining decision of this refactor: **the agent must not know channels
exist.** Channels are not agent-callable tools. They are *remote surfaces* — a
Telegram chat is "the TUI, but remote."

- An inbound message from any surface (TUI, Telegram, Discord, …) enters the
  agent exactly like a TUI user typing a prompt.
- The agent responds with its normal request/response loop. Nothing
  channel-specific is in its context — no `*_send` tools, no `*_connect` tools,
  no channel hints appended to the prompt or tool list.
- The gateway routes the response **back to the surface the request came from**,
  automatically — not because the agent chose a channel, but because that is
  where the conversation originated. This is the regular way a user chats with
  the model, just delivered over a remote transport.

TUI and every channel are **peer surfaces** on one bus. The agent is
surface-agnostic and purely reactive.

### Consequences

- ❌ Delete `telegram_send` / `whatsapp_send` / `discord_send` / `slack_send` /
  `trello_send` tools.
- ❌ Delete `telegram_connect` / … `_connect` tools from the agent's context.
  Connecting a channel becomes a config / onboarding action, not an agent tool
  call.
- ❌ Nothing channel-related appended to the agent prompt or tool list.
- ✅ The bus carries a normalized `Inbound { surface_id, conversation_key,
  sender, text, … }` → agent → `Outbound` routed back to `surface_id`.
- ✅ The `cli/ui.rs:798` resume-recovery match collapses into a generic "route
  the response to its origin surface" path.

## Architecture

New module tree under `src/channels/gateway/`:

```
src/channels/
  gateway/
    mod.rs          # public surface: Gateway, GatewayHandle, run loop
    surface.rs      # Surface trait + descriptor types
    envelope.rs     # Inbound / Outbound normalized message types
    bus.rs          # async bus (tokio mpsc) wiring inbound -> agent -> outbound
    registry.rs     # the ONLY place channel #[cfg(feature=...)] lives
    services/       # shared cross-cutting logic extracted from handlers
      allowlist.rs  # respond_to / allowed_users / allowed_channels
      session.rs    # (moves session_resolve.rs + session_init.rs here)
      approval.rs   # approval + follow_up_question orchestration
  telegram/ discord/ slack/ whatsapp/ trello/   # each implements Surface
```

### 1. The `Surface` trait (gateway contract)

Object-safe, async. Every surface (TUI + each channel) implements it.

```rust
#[async_trait]
pub trait Surface: Send + Sync {
    /// Stable id: "tui", "telegram", "discord", ...
    fn id(&self) -> &'static str;

    /// Whether this surface is enabled + has valid credentials right now.
    fn status(&self, cfg: &Config) -> SurfaceStatus;

    /// Start listening. The surface publishes inbound envelopes to `bus`
    /// and is handed an outbound receiver to deliver responses. Returns a
    /// JoinHandle for the listener task.
    async fn start(self: Arc<Self>, bus: GatewayHandle) -> JoinHandle<()>;

    /// Deliver an agent response back out this surface to `target`.
    async fn deliver(&self, target: &OutboundTarget, msg: &OutboundMessage)
        -> anyhow::Result<()>;
}
```

`SurfaceStatus` mirrors today's per-channel `should_run` checks (enabled flag +
credential validity). `OutboundTarget` carries the `conversation_key`
(platform-stable chat id / thread id) needed to route a reply.

### 2. Normalized envelopes

```rust
pub struct Inbound {
    pub surface_id: &'static str,
    pub conversation_key: String,   // platform-stable: chat id, channel id, phone
    pub sender: SenderRef,          // id + display name, for allowlist + context
    pub text: String,               // already-extracted user text
    pub display_text: Option<String>, // what the user literally typed (for DB/TUI)
    pub reply_ctx: Option<ReplyContext>,
    pub attachments: Vec<Attachment>, // images/audio refs the agent input wraps
}

pub struct Outbound {
    pub surface_id: &'static str,
    pub target: OutboundTarget,
    pub message: OutboundMessage,   // text (+ voice/image artifacts flagged)
}
```

Platform-specific parsing (serenity events, teloxide updates, WhatsApp proto)
produces an `Inbound`; the surface's `deliver` consumes an `Outbound`. Voice
synthesis and image-marker extraction move to a shared post-processing step in
the bus pipeline so each surface no longer re-implements them.

### 3. The bus / gateway loop

`GatewayHandle` is a cloneable producer end (tokio mpsc sender for inbound +
a registry of per-surface outbound senders).

Inbound pipeline (runs once, surface-agnostic):

1. Receive `Inbound`.
2. **allowlist**: apply `respond_to` / `allowed_users` / `allowed_channels`
   (shared service; identical logic across platforms today).
3. **session**: `resolve_or_create_channel_session` keyed by
   `surface_id + conversation_key` (the existing suffix-stable resolver, moved
   under `gateway/services/session.rs`).
4. Call `agent.send_message_with_tools_and_display(session_id, text,
   display_text, …, channel = surface_id, channel_chat_id = conversation_key)`.
   The agent signature already takes `channel: &str` — no agent change needed.
5. Post-process (voice synth / image markers) once, centrally.
6. Build `Outbound` with `surface_id` = origin, publish to that surface's
   outbound sender.

The owning surface's `deliver` sends the message back. This collapses the
`ui.rs:798` match and the per-channel inline "send the response" code into one
place.

### 4. The cfg-gated registry — the one source-exclusion point

```rust
pub fn registered_surfaces(deps: &SurfaceDeps) -> Vec<Arc<dyn Surface>> {
    let mut v: Vec<Arc<dyn Surface>> = Vec::new();
    v.push(tui::surface(deps));                       // always present
    #[cfg(feature = "telegram")] v.push(telegram::surface(deps));
    #[cfg(feature = "discord")]  v.push(discord::surface(deps));
    #[cfg(feature = "slack")]    v.push(slack::surface(deps));
    #[cfg(feature = "whatsapp")] v.push(whatsapp::surface(deps));
    #[cfg(feature = "trello")]   v.push(trello::surface(deps));
    v
}
```

This is the **only** place `#[cfg(feature = "...")]` for channels appears.
`ChannelManager`, `ChannelFactory`, `cli/ui.rs`, and the gateway loop all iterate
`registered_surfaces()` — no per-platform branches anywhere else. An OFF channel:

- contributes zero source (its `pub mod x` is `#[cfg(feature="x")]` in
  `channels/mod.rs`, unchanged);
- drops its client dependency (teloxide/serenity/… already feature-gated in
  `Cargo.toml`, unchanged);
- adds no registry entry, so no runtime cost.

Build toggles keep flowing through the existing
`build_toggles.toml` → `build.rs` → cargo-features path **unchanged**. The
`tool-*-send` / `tool-*-connect` features are removed (the tools are deleted),
so `build.rs` `TOGGLE_TO_FEATURES` and `tool_features.py` are simplified: each
channel toggle maps to just its own `feature` (e.g. `telegram → ["telegram"]`).

### 5. Shared gateway services (handler-internal refactor)

Extracted from the per-channel handlers into `gateway/services/`:

- **allowlist** — `respond_to` / `allowed_users` / `allowed_channels` decision.
- **session** — `session_resolve.rs` + `session_init.rs` moved here verbatim,
  re-exported.
- **approval / follow_up_question** — the orchestration is shared; each surface
  supplies a thin adapter for its native button surface (Discord components,
  Telegram inline keyboards, Slack blocks). The per-call
  `override_question_callback` / `override_approval_callback` plumbing in
  `messaging.rs` is reused; the gateway builds the callback and the surface
  renders/collects.

Platform-specific message *parsing* stays in each channel module. Only
cross-cutting logic moves out.

### 6. TUI as a surface

The TUI becomes `tui::surface` on the same bus:

- On user submit (today: `tui/app/messaging.rs:2041`,
  `tui/app/state.rs:1058` calling `send_message_with_tools_and_mode`), publish
  an `Inbound { surface_id: "tui", conversation_key: <session>, … }` to the bus
  instead of calling the agent directly.
- `deliver` for the TUI emits a `TuiEvent::ResponseComplete` (today's render
  path) instead of a network send.

This unifies routing: one pipeline serves TUI and channels identically. The
TUI's input capture and rendering widgets are untouched — only the
submit→agent→render wiring is rerouted through the bus.

## Migration path (vertical slice first)

1. **Gateway core** — build `gateway/` (Surface trait, envelope, bus, registry,
   services) with no surface migrated. Move `session_resolve.rs` /
   `session_init.rs` under `services/`. Compiles alongside existing code.
2. **TUI surface** — migrate the TUI onto the bus. Validates the bus end-to-end
   with the lowest-risk surface (no network, full local visibility).
3. **Telegram surface** — migrate Telegram fully (richest handler, validates the
   trait hardest). Delete `telegram_send` / `telegram_connect` tools, its
   bespoke `ChannelManager`/`ChannelFactory` wiring.
4. **Discord → Slack → WhatsApp → Trello** — each migrated and verified
   independently, deleting its `*_send` / `*_connect` tools as it goes.
5. **Cleanup** — delete `ChannelManager`'s per-channel methods, the
   `ui.rs:798` match, the `tool-*-send` / `tool-*-connect` features, and the
   now-dead factory args. Simplify `build.rs` / `tool_features.py` toggle maps.

Each step keeps the build green and the test suite (~20 channel tests in
`src/tests/`) passing.

## Risks / trade-offs

- **170KB telegram handler, 90KB slack handler, 55KB whatsapp store** are the
  riskiest to touch. Extracting shared services from them is where regressions
  hide — TDD + existing channel tests are the safety net. Parsing logic stays
  put; only the boundary (lifecycle, inbound publish, outbound deliver) changes.
- **Removing proactive channel sends**: the agent can no longer push to a
  channel unprompted (no cron-to-Telegram "good morning", no connect greeting
  via `telegram_send`). Per the reframe this is intended — every outbound is a
  *response* the gateway forwards to the originating surface, mirroring normal
  TUI chat. The connect-greeting becomes a gateway-emitted message on the new
  connection's conversation_key, not an agent tool call.
- **One bus hop of indirection** vs. direct calls — negligible at messaging
  latencies, and it is what makes "subscribe and talk through" real.

## Success criteria

- Agent context contains zero channel tools; `list_tools()` no longer includes
  any `*_send` / `*_connect`.
- Adding a new channel = implement `Surface` + one `#[cfg]` line in
  `registry.rs` + one Cargo feature. No edits to manager/factory/ui.rs.
- A channel toggled `false` in `build_toggles.toml` compiles out entirely
  (verified: `nm`/symbol check or `cargo bloat` shows no platform client code).
- TUI and channels share one inbound→agent→outbound path.
- All existing channel tests pass; new tests cover the bus routing and
  allowlist/session services.
