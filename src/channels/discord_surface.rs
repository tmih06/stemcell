//! The Discord surface — a Discord bot as a peer on the gateway bus.
//!
//! Follows the Telegram pattern: `status` derives from config (token validity),
//! `start` launches the existing serenity dispatcher (`DiscordAgent`) as the
//! inbound listener, and `deliver` posts an outbound response to a channel by
//! id. `conversation_key` for Discord is the channel id rendered as a string.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::task::JoinHandle;

use super::discord::DiscordState;
use super::gateway::bus::GatewayHandle;
use super::gateway::envelope::{OutboundMessage, OutboundTarget};
use super::gateway::registry::SurfaceDeps;
use super::gateway::surface::{Surface, SurfaceStatus};
use crate::config::Config;
use crate::db::ChannelMessageRepository;

/// The Discord surface.
pub struct DiscordSurface {
    state: Arc<DiscordState>,
    agent: Arc<crate::brain::agent::AgentService>,
    service_context: crate::services::ServiceContext,
    shared_session_id: Arc<tokio::sync::Mutex<Option<uuid::Uuid>>>,
    config_rx: tokio::sync::watch::Receiver<Config>,
    db_pool: deadpool_sqlite::Pool,
}

impl DiscordSurface {
    pub fn new(deps: &SurfaceDeps, state: Arc<DiscordState>) -> Self {
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

    /// Discord bot tokens are ~70 chars; the old reconcile used `len > 50`.
    fn token_is_valid(token: &str) -> bool {
        !token.is_empty() && token.len() > 50
    }
}

#[async_trait]
impl Surface for DiscordSurface {
    fn id(&self) -> &'static str {
        "discord"
    }

    fn status(&self, cfg: &Config) -> SurfaceStatus {
        let dc = &cfg.channels.discord;
        let has_valid_token = dc
            .token
            .as_deref()
            .map(Self::token_is_valid)
            .unwrap_or(false);
        if dc.enabled && has_valid_token {
            SurfaceStatus::Ready
        } else {
            SurfaceStatus::Inactive
        }
    }

    async fn start(self: Arc<Self>, _bus: GatewayHandle) -> JoinHandle<()> {
        let token = self.config_rx.borrow().channels.discord.token.clone();
        let Some(token) = token else {
            tracing::warn!("DiscordSurface::start called with no token configured");
            return tokio::spawn(async {});
        };
        let agent = crate::channels::discord::DiscordAgent::new(
            self.agent.clone(),
            self.service_context.clone(),
            self.shared_session_id.clone(),
            self.state.clone(),
            self.config_rx.clone(),
            ChannelMessageRepository::new(self.db_pool.clone()),
        );
        agent.start(token)
    }

    async fn deliver(
        &self,
        target: &OutboundTarget,
        message: &OutboundMessage,
    ) -> anyhow::Result<()> {
        let http =
            self.state.http().await.ok_or_else(|| {
                anyhow::anyhow!("Discord not connected — no HTTP client in state")
            })?;
        let channel_id: u64 = target.conversation_key.parse().map_err(|e| {
            anyhow::anyhow!(
                "invalid Discord channel id '{}': {e}",
                target.conversation_key
            )
        })?;
        let channel = serenity::model::id::ChannelId::new(channel_id);
        channel
            .say(&http, &message.text)
            .await
            .map_err(|e| anyhow::anyhow!("Discord send failed: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_validity_matches_reconcile_rules() {
        assert!(DiscordSurface::token_is_valid(&"x".repeat(60)));
        assert!(!DiscordSurface::token_is_valid("short"));
        assert!(!DiscordSurface::token_is_valid(""));
    }
}
