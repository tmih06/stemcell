//! The gateway bus: the single inbound→agent→outbound pipeline shared by every
//! surface.
//!
//! ## Flow
//!
//! 1. Each surface, on receiving a native message, builds an
//!    [`Inbound`](super::envelope::Inbound) and calls
//!    [`GatewayHandle::publish_inbound`].
//! 2. The gateway run loop (one task) receives the inbound, runs the shared
//!    pipeline — allowlist → session resolve → agent turn → post-process — and
//!    builds an [`Outbound`](super::envelope::Outbound) addressed back to the
//!    originating surface.
//! 3. The gateway looks up the owning surface by `surface_id` and calls its
//!    [`Surface::deliver`](super::surface::Surface::deliver).
//!
//! The agent never learns which surface a message came from beyond the opaque
//! `channel` string it already records — there are no channel tools, no
//! per-surface branches in the agent. "Subscribe and talk through" is literally
//! this: surfaces subscribe to the bus by registering, and talk through it by
//! publishing inbound envelopes.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;

use super::envelope::{Inbound, Outbound, OutboundMessage, OutboundTarget};
use super::services::allowlist::AllowlistDecision;
use super::surface::Surface;
use crate::brain::agent::AgentService;
use crate::config::Config;
use crate::services::SessionService;

/// Cloneable handle surfaces use to publish inbound messages onto the bus.
///
/// Handed to each surface in [`Surface::start`](super::surface::Surface::start).
/// Cloning is cheap (an `mpsc::Sender` clone).
#[derive(Clone)]
pub struct GatewayHandle {
    inbound_tx: mpsc::Sender<Inbound>,
}

impl GatewayHandle {
    /// Publish an inbound message to the gateway pipeline. Returns `false` if
    /// the message could not be enqueued — either the gateway loop has shut
    /// down (receiver dropped) or the bounded queue is full (backpressure). A
    /// full queue means the agent is not keeping up; dropping is preferable to
    /// growing memory without bound, and the surface can decide whether to
    /// retry or surface an error to the user.
    pub fn publish_inbound(&self, inbound: Inbound) -> bool {
        match self.inbound_tx.try_send(inbound) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(dropped)) => {
                tracing::warn!(
                    "gateway: inbound queue full, dropping {} message from conversation {}",
                    dropped.surface_id,
                    dropped.conversation_key
                );
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }
}

/// Everything the inbound pipeline needs to turn an [`Inbound`] into a
/// delivered response. Cloneable so each per-message task gets its own view.
#[derive(Clone)]
pub struct GatewayContext {
    pub agent: Arc<AgentService>,
    pub session_service: SessionService,
    pub config_rx: tokio::sync::watch::Receiver<Config>,
}

/// Shared, cloneable core of the gateway: the pipeline context plus the
/// registered surfaces keyed by id. Held behind an `Arc` so per-message tasks
/// can run concurrently.
struct Core {
    surfaces: HashMap<&'static str, Arc<dyn Surface>>,
    ctx: GatewayContext,
}

impl Core {
    /// Resolve an inbound message to the outbound response that should be
    /// delivered, or `None` when the message is dropped (allowlist reject,
    /// session error, or agent error).
    async fn process(&self, inbound: &Inbound) -> Option<Outbound> {
        let cfg = self.ctx.config_rx.borrow().clone();

        // A surface that resolved its own session has already applied its
        // platform gating before publishing (Telegram/Discord/Slack
        // respond_to, Trello @mention, WhatsApp allowed_phones), so re-running
        // the shared allowlist here would double-gate. The gateway re-applies
        // the shared allowlist only for surfaces that defer session resolution
        // to it (no hint).
        if inbound.session_hint.is_none()
            && let AllowlistDecision::Ignore { reason } =
                super::services::allowlist::evaluate(inbound, &cfg)
        {
            tracing::debug!(
                "gateway: ignoring {} message from {}: {}",
                inbound.surface_id,
                inbound.sender.id,
                reason
            );
            return None;
        }

        // 2. Resolve (or create) the session for this conversation. A surface
        //    that owns session semantics the keyed resolver can't reproduce
        //    supplies the id directly via `session_hint`.
        let session_id = match inbound.session_hint {
            Some(id) => id,
            None => match super::services::session::resolve_for_inbound(
                &self.ctx.session_service,
                inbound,
                &cfg,
            )
            .await
            {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!(
                        "gateway: session resolve failed for {} conversation {}: {}",
                        inbound.surface_id,
                        inbound.conversation_key,
                        e
                    );
                    return None;
                }
            },
        };

        // 3. Run the agent turn exactly like a TUI message — no channel tools,
        //    no per-surface branching. `channel` is the opaque surface id.
        //    The originating surface supplies its interactive callbacks
        //    (progress / approval / follow-up) so the shared loop renders on
        //    that surface's native UI.
        let cb = self
            .surfaces
            .get(inbound.surface_id)
            .map(|s| s.callbacks(&inbound.conversation_key, session_id))
            .unwrap_or_default();

        let response = match self
            .ctx
            .agent
            .send_message_with_tools_and_display(
                session_id,
                inbound.text.clone(),
                inbound.display_text.clone(),
                None,
                cb.cancel_token.clone(),
                cb.approval,
                cb.progress,
                cb.question,
                inbound.surface_id,
                Some(inbound.conversation_key.as_str()),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(
                    "gateway: agent turn failed for session {}: {}",
                    session_id,
                    e
                );
                return None;
            }
        };

        // 4. Post-process once, centrally: extract image markers so every
        //    surface receives the same cleaned text + image list.
        let (clean_text, images) = crate::utils::image::extract_img_markers(&response.content);

        Some(Outbound {
            surface_id: inbound.surface_id,
            target: OutboundTarget {
                conversation_key: inbound.conversation_key.clone(),
                thread_key: inbound
                    .reply_ctx
                    .as_ref()
                    .and_then(|r| r.message_id.clone()),
            },
            message: OutboundMessage {
                text: clean_text,
                session_id,
                images,
                full: Arc::new(response),
            },
        })
    }

    /// Deliver an outbound response to its originating surface.
    async fn deliver(&self, outbound: Outbound) {
        let Some(surface) = self.surfaces.get(outbound.surface_id) else {
            tracing::warn!(
                "gateway: no registered surface '{}' to deliver response — dropping",
                outbound.surface_id
            );
            return;
        };
        if let Err(e) = surface.deliver(&outbound.target, &outbound.message).await {
            tracing::error!(
                "gateway: surface '{}' failed to deliver response: {}",
                outbound.surface_id,
                e
            );
        }
    }
}

/// Capacity of the bounded inbound queue. Sized to absorb normal bursts (many
/// conversations sending at once) while still applying backpressure — once the
/// agent falls this far behind, [`GatewayHandle::publish_inbound`] drops rather
/// than letting memory grow without bound.
const INBOUND_QUEUE_CAPACITY: usize = 256;

/// The running gateway: owns the inbound receiver and the shared [`Core`].
pub struct Gateway {
    inbound_rx: mpsc::Receiver<Inbound>,
    handle: GatewayHandle,
    core: Arc<Core>,
    /// Listener tasks for surfaces that have been started, keyed by surface id.
    /// Mirrors `ChannelManager`'s handle map so a surface is started once and
    /// can be stopped on config-driven disable.
    listeners: HashMap<&'static str, tokio::task::JoinHandle<()>>,
}

impl Gateway {
    /// Create a gateway with the given pipeline context and registered
    /// surfaces. The surfaces come from the cfg-gated
    /// [`registry`](super::registry) so an off channel is simply absent here.
    pub fn new(ctx: GatewayContext, surfaces: Vec<Arc<dyn Surface>>) -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(INBOUND_QUEUE_CAPACITY);
        let map = surfaces.into_iter().map(|s| (s.id(), s)).collect();
        Self {
            inbound_rx,
            handle: GatewayHandle { inbound_tx },
            core: Arc::new(Core { surfaces: map, ctx }),
            listeners: HashMap::new(),
        }
    }

    /// A handle for publishing inbound messages. Clone and hand to surfaces.
    pub fn handle(&self) -> GatewayHandle {
        self.handle.clone()
    }

    /// Resolve an inbound to its outbound response without delivering it.
    /// Exposed for unit tests that exercise the pipeline directly.
    pub async fn process(&self, inbound: Inbound) -> Option<Outbound> {
        self.core.process(&inbound).await
    }

    /// Start (or stop) each surface to match the current config — the
    /// generic replacement for `ChannelManager::reconcile`'s per-channel
    /// `should_run` / `is_running` dance. A surface reporting
    /// [`SurfaceStatus::Ready`](super::surface::SurfaceStatus::Ready) that
    /// isn't running is started; one that is running but no longer ready is
    /// aborted. Idempotent: safe to call on every config reload.
    pub async fn reconcile(&mut self, cfg: &Config) {
        for (id, surface) in &self.core.surfaces {
            let ready = surface.status(cfg).is_ready();
            let running = self
                .listeners
                .get(id)
                .map(|h| !h.is_finished())
                .unwrap_or(false);

            if ready && !running {
                tracing::info!("gateway: starting surface '{}'", id);
                let handle = surface.clone().start(self.handle.clone()).await;
                self.listeners.insert(id, handle);
            } else if !ready
                && running
                && let Some(handle) = self.listeners.remove(id)
            {
                tracing::info!("gateway: stopping surface '{}'", id);
                handle.abort();
            }
        }
    }

    /// Run the inbound→agent→outbound loop until all inbound senders drop.
    ///
    /// Messages in **different** conversations are processed concurrently (each
    /// on its own spawned task) so a slow agent turn on one conversation doesn't
    /// block others. Messages in the **same** conversation are serialized in
    /// arrival order: each task awaits the previous task for its conversation
    /// before running, so two quick messages in one chat can't have their agent
    /// turns or deliveries interleave/reorder. The chain is keyed on
    /// `(surface_id, conversation_key)`.
    ///
    /// The loop also watches the config watch channel and re-[`reconcile`]s
    /// surfaces on change, so toggling `channels.telegram.enabled` at runtime
    /// starts/stops the surface without a restart — the gateway equivalent of
    /// the old `ChannelManager` config-reload callback.
    pub async fn run(mut self) {
        let mut config_rx = self.core.ctx.config_rx.clone();
        // Tail task per conversation. A new message for a conversation awaits
        // the existing tail before processing, preserving per-conversation
        // FIFO. Owned solely by this loop task, so no locking is needed.
        let mut chains: HashMap<(&'static str, String), tokio::task::JoinHandle<()>> =
            HashMap::new();
        loop {
            tokio::select! {
                maybe_inbound = self.inbound_rx.recv() => {
                    let Some(inbound) = maybe_inbound else {
                        tracing::info!("gateway: inbound channel closed, run loop exiting");
                        return;
                    };
                    let core = self.core.clone();
                    let key = (inbound.surface_id, inbound.conversation_key.clone());
                    // Chain onto this conversation's previous task (if any) so
                    // processing is strictly ordered within the conversation.
                    let prev = chains.remove(&key);
                    let task = tokio::spawn(async move {
                        if let Some(prev) = prev {
                            // Ignore a panicked predecessor — we still want this
                            // message processed; ordering is best-effort across
                            // a crash, strict otherwise.
                            let _ = prev.await;
                        }
                        if let Some(outbound) = core.process(&inbound).await {
                            core.deliver(outbound).await;
                        }
                    });
                    chains.insert(key, task);
                    // Drop completed chain tails so the map doesn't grow with
                    // the number of conversations ever seen.
                    chains.retain(|_, h| !h.is_finished());
                }
                changed = config_rx.changed() => {
                    if changed.is_err() {
                        // Config sender dropped — keep serving inbound, just
                        // stop watching for config changes.
                        continue;
                    }
                    let cfg = config_rx.borrow_and_update().clone();
                    self.reconcile(&cfg).await;
                }
            }
        }
    }
}
