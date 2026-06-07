//! The Telegram surface — a Telegram bot as a peer on the gateway bus.
//!
//! Telegram is the first channel migrated onto the [`Surface`] trait. It reuses
//! the existing, battle-tested teloxide dispatcher (`telegram::TelegramAgent`)
//! as its inbound listener — that 170KB handler does Telegram-specific work
//! (photo batching, voice transcription, forum-topic routing, group history,
//! slash commands) that produces the wrapped agent input. Rather than rewrite
//! it blind, the surface wraps it:
//!
//! - [`status`](TelegramSurface::status) — derived from config exactly like the
//!   old `ChannelManager::reconcile_telegram` token-validity check.
//! - [`start`](TelegramSurface::start) — launches the existing dispatcher.
//! - [`deliver`](TelegramSurface::deliver) — proactive/outbound send, reusing
//!   the thread-aware `send::message_in_thread` helper so forum-topic replies
//!   land in the right topic (#130).
//!
//! `conversation_key` for Telegram is the chat id rendered as a string.

use std::sync::Arc;

use async_trait::async_trait;
use teloxide::types::ChatId;
use tokio::task::JoinHandle;

use super::gateway::bus::GatewayHandle;
use super::gateway::envelope::{OutboundMessage, OutboundTarget};
use super::gateway::registry::SurfaceDeps;
use super::gateway::surface::{Surface, SurfaceStatus};
use super::telegram::TelegramState;
use crate::config::Config;
use crate::db::ChannelMessageRepository;

/// The Telegram surface. Holds the shared [`TelegramState`] (the connected
/// `Bot`, owner chat id, approval/question routing) plus the deps needed to
/// spawn the dispatcher.
pub struct TelegramSurface {
    state: Arc<TelegramState>,
    agent: Arc<crate::brain::agent::AgentService>,
    service_context: crate::services::ServiceContext,
    shared_session_id: Arc<tokio::sync::Mutex<Option<uuid::Uuid>>>,
    config_rx: tokio::sync::watch::Receiver<Config>,
    db_pool: deadpool_sqlite::Pool,
}

impl TelegramSurface {
    /// Construct from the shared surface deps and the Telegram state.
    pub fn new(deps: &SurfaceDeps, state: Arc<TelegramState>) -> Self {
        Self {
            state,
            agent: deps.agent.clone(),
            service_context: deps.service_context.clone(),
            shared_session_id: deps.shared_session_id.clone(),
            config_rx: deps.config_rx.clone(),
            db_pool: deps.db_pool.clone(),
        }
    }

    /// Wrap in an `Arc` for registry insertion.
    pub fn into_arc(self) -> Arc<dyn Surface> {
        Arc::new(self)
    }

    /// Token-validity check, mirroring the old `reconcile_telegram` logic:
    /// "numbers:alphanumeric", with the API-key half at least 30 chars.
    fn token_is_valid(token: &str) -> bool {
        if token.is_empty() || !token.contains(':') {
            return false;
        }
        let parts: Vec<&str> = token.splitn(2, ':').collect();
        parts.len() == 2 && parts[0].parse::<u64>().is_ok() && parts[1].len() >= 30
    }
}

#[async_trait]
impl Surface for TelegramSurface {
    fn id(&self) -> &'static str {
        "telegram"
    }

    fn status(&self, cfg: &Config) -> SurfaceStatus {
        let tg = &cfg.channels.telegram;
        let has_valid_token = tg
            .token
            .as_deref()
            .map(Self::token_is_valid)
            .unwrap_or(false);
        if tg.enabled && has_valid_token {
            SurfaceStatus::Ready
        } else {
            SurfaceStatus::Inactive
        }
    }

    async fn start(self: Arc<Self>, _bus: GatewayHandle) -> JoinHandle<()> {
        // Reuse the existing dispatcher as the inbound listener. It performs the
        // Telegram-specific preprocessing and currently drives the agent loop
        // directly; routing its inbound through the bus is a follow-up step that
        // requires threading `GatewayHandle` into the handler. For now the
        // surface owns lifecycle + outbound delivery while the proven inbound
        // path is preserved verbatim.
        let token = self.config_rx.borrow().channels.telegram.token.clone();
        let Some(token) = token else {
            tracing::warn!("TelegramSurface::start called with no token configured");
            return tokio::spawn(async {});
        };

        let agent = crate::channels::telegram::TelegramAgent::new(
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
        let bot = self
            .state
            .bot()
            .await
            .ok_or_else(|| anyhow::anyhow!("Telegram not connected — no Bot in state"))?;

        let chat_id: i64 = target.conversation_key.parse().map_err(|e| {
            anyhow::anyhow!(
                "invalid Telegram chat id '{}': {e}",
                target.conversation_key
            )
        })?;

        // Resolve the forum topic: an explicit thread_key wins, otherwise fall
        // back to the most recent topic seen for this chat (#130).
        let thread_id = match target
            .thread_key
            .as_deref()
            .and_then(|s| s.parse::<i32>().ok())
        {
            Some(tid) => Some(teloxide::types::ThreadId(teloxide::types::MessageId(tid))),
            None => crate::channels::telegram::send::latest_thread_id_for_chat(chat_id).await,
        };

        // Split into Telegram's 4096-char limit and send each chunk in-thread.
        let chunks = crate::channels::telegram::handler::split_message(&message.text, 4096);
        for chunk in chunks {
            crate::channels::telegram::send::message_in_thread(
                &bot,
                ChatId(chat_id),
                thread_id,
                chunk,
            )
            .await
            .map_err(|e| anyhow::anyhow!("Telegram send failed: {e}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_validity_matches_reconcile_rules() {
        // Valid: numeric id + ':' + >=30 char key.
        assert!(TelegramSurface::token_is_valid(&format!(
            "123456789:{}",
            "A".repeat(30)
        )));
        // Invalid: no colon.
        assert!(!TelegramSurface::token_is_valid("123456789ABC"));
        // Invalid: non-numeric id.
        assert!(!TelegramSurface::token_is_valid(&format!(
            "notanid:{}",
            "A".repeat(30)
        )));
        // Invalid: key too short.
        assert!(!TelegramSurface::token_is_valid("123456789:short"));
        // Invalid: empty.
        assert!(!TelegramSurface::token_is_valid(""));
    }
}
