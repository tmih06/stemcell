//! The Slack surface — a Slack bot (Socket Mode) as a peer on the gateway bus.
//!
//! Follows the Telegram pattern. Slack needs two tokens (bot `xoxb-` + app
//! `xapp-`); `status` validates both. `start` launches the existing
//! Socket-Mode dispatcher (`SlackAgent`). `deliver` posts a message to a
//! channel via the stored client. `conversation_key` is the Slack channel id.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::task::JoinHandle;

use super::gateway::bus::GatewayHandle;
use super::gateway::envelope::{OutboundMessage, OutboundTarget};
use super::gateway::registry::SurfaceDeps;
use super::gateway::surface::{Surface, SurfaceStatus};
use super::slack::SlackState;
use crate::config::Config;
use crate::db::ChannelMessageRepository;

/// The Slack surface.
pub struct SlackSurface {
    state: Arc<SlackState>,
    agent: Arc<crate::brain::agent::AgentService>,
    service_context: crate::services::ServiceContext,
    shared_session_id: Arc<tokio::sync::Mutex<Option<uuid::Uuid>>>,
    config_rx: tokio::sync::watch::Receiver<Config>,
    db_pool: deadpool_sqlite::Pool,
}

impl SlackSurface {
    pub fn new(deps: &SurfaceDeps, state: Arc<SlackState>) -> Self {
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
impl Surface for SlackSurface {
    fn id(&self) -> &'static str {
        "slack"
    }

    fn status(&self, cfg: &Config) -> SurfaceStatus {
        let sl = &cfg.channels.slack;
        let bot_ok = sl
            .token
            .as_deref()
            .map(|t| !t.is_empty() && t.starts_with("xoxb-"))
            .unwrap_or(false);
        let app_ok = sl
            .app_token
            .as_deref()
            .map(|t| !t.is_empty() && t.starts_with("xapp-"))
            .unwrap_or(false);
        if sl.enabled && bot_ok && app_ok {
            SurfaceStatus::Ready
        } else {
            SurfaceStatus::Inactive
        }
    }

    async fn start(self: Arc<Self>, _bus: GatewayHandle) -> JoinHandle<()> {
        let (bot_token, app_token) = {
            let cfg = self.config_rx.borrow();
            (
                cfg.channels.slack.token.clone(),
                cfg.channels.slack.app_token.clone(),
            )
        };
        let (Some(bot_token), Some(app_token)) = (bot_token, app_token) else {
            tracing::warn!("SlackSurface::start called with missing token(s)");
            return tokio::spawn(async {});
        };
        let agent = crate::channels::slack::SlackAgent::new(
            self.agent.clone(),
            self.service_context.clone(),
            self.shared_session_id.clone(),
            self.state.clone(),
            self.config_rx.clone(),
            ChannelMessageRepository::new(self.db_pool.clone()),
        );
        agent.start(bot_token, app_token)
    }

    async fn deliver(
        &self,
        target: &OutboundTarget,
        message: &OutboundMessage,
    ) -> anyhow::Result<()> {
        use slack_morphism::prelude::*;

        let token_val = self
            .state
            .bot_token()
            .await
            .ok_or_else(|| anyhow::anyhow!("Slack not connected — no bot token in state"))?;
        let client = self
            .state
            .client()
            .await
            .ok_or_else(|| anyhow::anyhow!("Slack not connected — no client in state"))?;

        let api_token = SlackApiToken::new(SlackApiTokenValue::from(token_val));
        let session = client.open_session(&api_token);
        let req = SlackApiChatPostMessageRequest::new(
            target.conversation_key.clone().into(),
            SlackMessageContent::new().with_text(message.text.clone()),
        );
        session
            .chat_post_message(&req)
            .await
            .map_err(|e| anyhow::anyhow!("Slack send failed: {e}"))?;
        Ok(())
    }
}
