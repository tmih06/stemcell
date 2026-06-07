//! The WhatsApp surface — the WhatsApp bot as a peer on the gateway bus.
//!
//! WhatsApp has no API token (it pairs via QR code), so `status` is just the
//! enabled flag — the agent always starts when enabled and emits QR events for
//! onboarding if unpaired. `start` launches the existing `WhatsAppAgent`.
//! `deliver` sends a message to a JID via the stored client. `conversation_key`
//! is the JID string.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::task::JoinHandle;

use super::gateway::bus::GatewayHandle;
use super::gateway::envelope::{OutboundMessage, OutboundTarget};
use super::gateway::registry::SurfaceDeps;
use super::gateway::surface::{Surface, SurfaceStatus};
use super::whatsapp::WhatsAppState;
use crate::config::Config;
use crate::db::ChannelMessageRepository;

/// The WhatsApp surface.
pub struct WhatsAppSurface {
    state: Arc<WhatsAppState>,
    agent: Arc<crate::brain::agent::AgentService>,
    service_context: crate::services::ServiceContext,
    shared_session_id: Arc<tokio::sync::Mutex<Option<uuid::Uuid>>>,
    config_rx: tokio::sync::watch::Receiver<Config>,
    db_pool: deadpool_sqlite::Pool,
}

impl WhatsAppSurface {
    pub fn new(deps: &SurfaceDeps, state: Arc<WhatsAppState>) -> Self {
        Self {
            state,
            agent: deps.agent.clone(),
            service_context: deps.service_context.clone(),
            shared_session_id: deps.shared_session_id.clone(),
            config_rx: deps.config_rx.clone(),
            db_pool: deps.db_pool.clone(),
        }
    }

    pub fn into_arc(self) -> Arc<dyn Surface> {
        Arc::new(self)
    }
}

#[async_trait]
impl Surface for WhatsAppSurface {
    fn id(&self) -> &'static str {
        "whatsapp"
    }

    fn status(&self, cfg: &Config) -> SurfaceStatus {
        // No token — pairing is via QR. Ready whenever enabled.
        if cfg.channels.whatsapp.enabled {
            SurfaceStatus::Ready
        } else {
            SurfaceStatus::Inactive
        }
    }

    async fn start(self: Arc<Self>, _bus: GatewayHandle) -> JoinHandle<()> {
        let agent = crate::channels::whatsapp::WhatsAppAgent::new(
            self.agent.clone(),
            self.service_context.clone(),
            self.shared_session_id.clone(),
            self.state.clone(),
            self.config_rx.clone(),
            ChannelMessageRepository::new(self.db_pool.clone()),
        );
        agent.start()
    }

    async fn deliver(
        &self,
        target: &OutboundTarget,
        message: &OutboundMessage,
    ) -> anyhow::Result<()> {
        let client = self
            .state
            .client()
            .await
            .ok_or_else(|| anyhow::anyhow!("WhatsApp not connected — no client in state"))?;
        let jid = target
            .conversation_key
            .parse::<wacore_binary::jid::Jid>()
            .map_err(|e| {
                anyhow::anyhow!("invalid WhatsApp JID '{}': {e}", target.conversation_key)
            })?;
        let msg = waproto::whatsapp::Message {
            conversation: Some(message.text.clone()),
            ..Default::default()
        };
        client
            .send_message(jid, msg)
            .await
            .map_err(|e| anyhow::anyhow!("WhatsApp send failed: {e}"))?;
        Ok(())
    }
}
