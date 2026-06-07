//! The Trello surface — the Trello board poller as a peer on the gateway bus.
//!
//! Trello has no inbound websocket: the `TrelloAgent` polls boards for new card
//! comments. `status` requires credentials (api key + token) and at least one
//! board. `start` launches the poller with the config's board ids / interval.
//! `deliver` posts a comment back on a card via a fresh `TrelloClient` built
//! from the stored credentials. `conversation_key` is the Trello card id.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::task::JoinHandle;

use super::gateway::bus::GatewayHandle;
use super::gateway::envelope::{OutboundMessage, OutboundTarget};
use super::gateway::registry::SurfaceDeps;
use super::gateway::surface::{Surface, SurfaceStatus};
use super::trello::TrelloState;
use crate::config::Config;

/// The Trello surface.
pub struct TrelloSurface {
    state: Arc<TrelloState>,
    agent: Arc<crate::brain::agent::AgentService>,
    service_context: crate::services::ServiceContext,
    shared_session_id: Arc<tokio::sync::Mutex<Option<uuid::Uuid>>>,
    config_rx: tokio::sync::watch::Receiver<Config>,
}

impl TrelloSurface {
    pub fn new(deps: &SurfaceDeps, state: Arc<TrelloState>) -> Self {
        Self {
            state,
            agent: deps.agent.clone(),
            service_context: deps.service_context.clone(),
            shared_session_id: deps.shared_session_id.clone(),
            config_rx: deps.config_rx.clone(),
        }
    }

    pub fn into_arc(self) -> Arc<dyn Surface> {
        Arc::new(self)
    }
}

#[async_trait]
impl Surface for TrelloSurface {
    fn id(&self) -> &'static str {
        "trello"
    }

    fn status(&self, cfg: &Config) -> SurfaceStatus {
        let tr = &cfg.channels.trello;
        let has_creds = tr
            .app_token
            .as_deref()
            .map(|k| !k.is_empty())
            .unwrap_or(false)
            && tr.token.as_deref().map(|t| !t.is_empty()).unwrap_or(false);
        let has_boards = !tr.board_ids.is_empty();
        if tr.enabled && has_creds && has_boards {
            SurfaceStatus::Ready
        } else {
            SurfaceStatus::Inactive
        }
    }

    async fn start(self: Arc<Self>, _bus: GatewayHandle) -> JoinHandle<()> {
        let (api_key, api_token, board_ids, poll_interval_secs, idle_hours, allowed_users) = {
            let cfg = self.config_rx.borrow();
            let tr = &cfg.channels.trello;
            (
                tr.app_token.clone(),
                tr.token.clone(),
                tr.board_ids.clone(),
                tr.poll_interval_secs,
                tr.session_idle_hours,
                tr.allowed_users.clone(),
            )
        };
        let (Some(api_key), Some(api_token)) = (api_key, api_token) else {
            tracing::warn!("TrelloSurface::start called with missing credentials");
            return tokio::spawn(async {});
        };
        let agent = crate::channels::trello::TrelloAgent::new(
            self.agent.clone(),
            self.service_context.clone(),
            allowed_users,
            self.shared_session_id.clone(),
            self.state.clone(),
            board_ids,
            poll_interval_secs,
            idle_hours,
        );
        agent.start(api_key, api_token)
    }

    async fn deliver(
        &self,
        target: &OutboundTarget,
        message: &OutboundMessage,
    ) -> anyhow::Result<()> {
        let (api_key, api_token) = self
            .state
            .credentials()
            .await
            .ok_or_else(|| anyhow::anyhow!("Trello not connected — no credentials in state"))?;
        let client = crate::channels::trello::TrelloClient::new(&api_key, &api_token);
        client
            .add_comment_to_card(&target.conversation_key, &message.text)
            .await
            .map_err(|e| anyhow::anyhow!("Trello comment failed: {e}"))?;
        Ok(())
    }
}
