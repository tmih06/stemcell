//! The [`Surface`] trait — the gateway's contract that every messaging
//! frontend (TUI and each channel) implements.
//!
//! A surface is a remote (or local) place a user talks to the agent. It knows
//! how to listen for inbound messages and publish them to the bus, and how to
//! deliver an agent response back to a conversation. It does **not** know
//! anything about the agent loop, sessions, or other surfaces — that lives in
//! the gateway. This is what lets the agent stay surface-agnostic: every
//! surface looks identical from the agent's side.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::task::JoinHandle;

use super::bus::GatewayHandle;
use super::envelope::{OutboundMessage, OutboundTarget};
use crate::config::Config;

/// Whether a surface should currently be running, derived from config. Mirrors
/// the per-channel `should_run` checks that `ChannelManager` did inline today
/// (enabled flag + credential validity).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceStatus {
    /// Enabled and ready to start (credentials valid where applicable).
    Ready,
    /// Present in the build but switched off or missing credentials.
    Inactive,
}

impl SurfaceStatus {
    pub fn is_ready(self) -> bool {
        matches!(self, SurfaceStatus::Ready)
    }
}

/// A frontend the agent can be reached through. Object-safe so the gateway can
/// hold `Vec<Arc<dyn Surface>>` from the cfg-gated registry.
#[async_trait]
pub trait Surface: Send + Sync {
    /// Stable identifier: `"tui"`, `"telegram"`, `"discord"`, … Matches the
    /// `channel` string already threaded through `AgentService` and the
    /// `channel_messages` table.
    fn id(&self) -> &'static str;

    /// Whether this surface should be running given the current config.
    fn status(&self, cfg: &Config) -> SurfaceStatus;

    /// Begin listening. The surface spawns its own task(s), publishing
    /// [`Inbound`](super::envelope::Inbound) envelopes to `bus` as messages
    /// arrive. Returns the listener `JoinHandle` so the gateway can abort it on
    /// shutdown or config-driven stop.
    async fn start(self: Arc<Self>, bus: GatewayHandle) -> JoinHandle<()>;

    /// Deliver an agent response back out this surface to `target`.
    async fn deliver(
        &self,
        target: &OutboundTarget,
        message: &OutboundMessage,
    ) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_status_is_ready() {
        assert!(SurfaceStatus::Ready.is_ready());
        assert!(!SurfaceStatus::Inactive.is_ready());
    }

    // Compile-time proof that `dyn Surface` is object-safe: if this builds,
    // the trait can be stored as a trait object in the registry.
    #[allow(dead_code)]
    fn assert_object_safe(_: &dyn Surface) {}
}
