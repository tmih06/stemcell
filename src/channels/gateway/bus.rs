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
    inbound_tx: mpsc::UnboundedSender<Inbound>,
}

impl GatewayHandle {
    /// Publish an inbound message to the gateway pipeline. Returns `false` if
    /// the gateway loop has shut down (receiver dropped).
    pub fn publish_inbound(&self, inbound: Inbound) -> bool {
        self.inbound_tx.send(inbound).is_ok()
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

        // 1. Allowlist — drop messages the surface's config says to ignore.
        if let AllowlistDecision::Ignore { reason } =
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

        // 2. Resolve (or create) the session for this conversation.
        let session_id = match super::services::session::resolve_for_inbound(
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
        };

        // 3. Run the agent turn exactly like a TUI message — no channel tools,
        //    no per-surface branching. `channel` is the opaque surface id.
        let response = match self
            .ctx
            .agent
            .send_message_with_tools_and_display(
                session_id,
                inbound.text.clone(),
                inbound.display_text.clone(),
                None,
                None,
                None,
                None,
                None,
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

/// The running gateway: owns the inbound receiver and the shared [`Core`].
pub struct Gateway {
    inbound_rx: mpsc::UnboundedReceiver<Inbound>,
    handle: GatewayHandle,
    core: Arc<Core>,
}

impl Gateway {
    /// Create a gateway with the given pipeline context and registered
    /// surfaces. The surfaces come from the cfg-gated
    /// [`registry`](super::registry) so an off channel is simply absent here.
    pub fn new(ctx: GatewayContext, surfaces: Vec<Arc<dyn Surface>>) -> Self {
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
        let map = surfaces.into_iter().map(|s| (s.id(), s)).collect();
        Self {
            inbound_rx,
            handle: GatewayHandle { inbound_tx },
            core: Arc::new(Core { surfaces: map, ctx }),
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

    /// Run the inbound→agent→outbound loop until all inbound senders drop.
    /// Each message is processed on its own spawned task so a slow agent turn
    /// on one conversation doesn't block others.
    pub async fn run(mut self) {
        while let Some(inbound) = self.inbound_rx.recv().await {
            let core = self.core.clone();
            tokio::spawn(async move {
                if let Some(outbound) = core.process(&inbound).await {
                    core.deliver(outbound).await;
                }
            });
        }
        tracing::info!("gateway: inbound channel closed, run loop exiting");
    }
}
