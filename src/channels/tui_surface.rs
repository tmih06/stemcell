//! The TUI surface — the local terminal frontend as a peer on the gateway bus.
//!
//! Modeling the TUI as a [`Surface`] is what makes the gateway truly unified:
//! the same inbound→agent→outbound pipeline serves the terminal and every
//! remote channel identically.
//!
//! - **conversation_key** for the TUI is the session id rendered as a string;
//!   [`session::resolve_for_inbound`](crate::channels::gateway::services::session::resolve_for_inbound)
//!   parses it straight back to the session with no DB lookup.
//! - **inbound** is published from the TUI submit path (wired in `cli/ui.rs`),
//!   not from a network listener — so [`start`](TuiSurface::start) spawns a
//!   no-op keepalive task; the surface's real job is delivery.
//! - **deliver** emits a [`TuiEvent::ResponseComplete`] carrying the full
//!   [`AgentResponse`], exactly as the pre-gateway code path did, so the TUI's
//!   renderer (footer cost/tokens/model, message list) is unchanged.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use super::gateway::bus::GatewayHandle;
use super::gateway::envelope::{OutboundMessage, OutboundTarget};
use super::gateway::registry::SurfaceDeps;
use super::gateway::surface::{Surface, SurfaceStatus};
use crate::config::Config;
use crate::tui::events::TuiEvent;

/// The local terminal surface. Holds the TUI event sender so agent responses
/// routed back by the gateway are rendered as `ResponseComplete` events.
pub struct TuiSurface {
    event_tx: UnboundedSender<TuiEvent>,
}

impl TuiSurface {
    /// Construct from the shared surface deps. The TUI event sender lives on
    /// [`SurfaceDeps::tui_event_tx`] — taking it here (rather than attaching it
    /// in a later builder step) means the surface can never be left holding a
    /// dead channel.
    pub fn new(deps: &SurfaceDeps) -> Self {
        Self {
            event_tx: deps.tui_event_tx.clone(),
        }
    }

    /// Wrap in an `Arc` for registry insertion.
    pub fn into_arc(self) -> Arc<dyn Surface> {
        Arc::new(self)
    }
}

#[async_trait]
impl Surface for TuiSurface {
    fn id(&self) -> &'static str {
        "tui"
    }

    fn status(&self, _cfg: &Config) -> SurfaceStatus {
        // The terminal frontend is always active.
        SurfaceStatus::Ready
    }

    async fn start(self: Arc<Self>, _bus: GatewayHandle) -> JoinHandle<()> {
        // The TUI publishes inbound from its own input/submit path rather than
        // a network listener, so there is nothing to poll here. Spawn an
        // immediately-finished task to satisfy the trait contract.
        tokio::spawn(async {})
    }

    async fn deliver(
        &self,
        target: &OutboundTarget,
        message: &OutboundMessage,
    ) -> anyhow::Result<()> {
        // Route the agent response back to the terminal renderer. The session
        // id comes from the outbound message; `target.conversation_key` is the
        // same session id in string form.
        let _ = target;
        self.event_tx
            .send(TuiEvent::ResponseComplete {
                session_id: message.session_id,
                response: (*message.full).clone(),
            })
            .map_err(|e| anyhow::anyhow!("TUI event channel closed: {e}"))?;
        Ok(())
    }
}
