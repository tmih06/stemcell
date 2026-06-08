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

    async fn start(self: Arc<Self>, bus: GatewayHandle) -> JoinHandle<()> {
        let agent = crate::channels::whatsapp::WhatsAppAgent::new(
            self.agent.clone(),
            self.service_context.clone(),
            self.shared_session_id.clone(),
            self.state.clone(),
            self.config_rx.clone(),
            ChannelMessageRepository::new(self.db_pool.clone()),
            bus,
        );
        agent.start()
    }

    fn callbacks(
        &self,
        _conversation_key: &str,
        session_id: uuid::Uuid,
    ) -> crate::channels::gateway::surface::SurfaceCallbacks {
        // WhatsApp's approval + follow-up-question callbacks are keyed on the
        // sender phone and target a chat JID — neither is on the generic
        // `(conversation_key, session_id)` signature. The listener stashed both
        // (plus the client is reachable via state) in the per-chat delivery
        // context, so the surface constructors read them from there.
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let state = self.state.clone();
        let token = cancel_token.clone();
        tokio::spawn(async move {
            state.store_cancel_token(session_id, token).await;
        });

        crate::channels::gateway::surface::SurfaceCallbacks {
            approval: Some(
                crate::channels::whatsapp::handler::make_surface_approval_callback(
                    self.state.clone(),
                ),
            ),
            progress: None,
            question: Some(
                crate::channels::whatsapp::follow_up_question::make_surface_question_callback(
                    self.state.clone(),
                ),
            ),
            cancel_token: Some(cancel_token),
        }
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

        let dctx = self.state.take_delivery_context(message.session_id).await;
        let is_voice = dctx.as_ref().map(|d| d.is_voice).unwrap_or(false);
        let is_group = dctx.as_ref().map(|d| d.is_group).unwrap_or(false);

        crate::channels::whatsapp::handler::deliver_reply(
            &client,
            jid,
            &message.text,
            is_group,
            is_voice,
            dctx.as_ref().map(|d| &d.voice_config),
            self.agent.context_limit_for_session(message.session_id),
            &message.full,
            &ChannelMessageRepository::new(self.db_pool.clone()),
        )
        .await;

        self.state.remove_cancel_token(message.session_id).await;
        Ok(())
    }
}
