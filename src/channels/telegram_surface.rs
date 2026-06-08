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

    async fn start(self: Arc<Self>, bus: GatewayHandle) -> JoinHandle<()> {
        // The existing teloxide dispatcher is the inbound listener: it does the
        // Telegram-specific preprocessing (photo batching, voice transcription,
        // forum-topic routing, group history, slash commands) + gating + session
        // resolution, then publishes an `Inbound` onto `bus`. The gateway runs
        // the agent turn and routes the response back through `deliver`.
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
            bus,
        );
        agent.start(token)
    }

    fn callbacks(
        &self,
        _conversation_key: &str,
        session_id: uuid::Uuid,
    ) -> crate::channels::gateway::surface::SurfaceCallbacks {
        // Rebuild Telegram's native interactive callbacks from shared state.
        // Approval keyboards and follow-up-question buttons resolve the chat
        // from `telegram_state.session_chat(session_id)`, so they reconstruct
        // without the original message. The cancel token is registered in
        // `telegram_state` so a `/stop` command can abort this turn.
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let state = self.state.clone();
        let token = cancel_token.clone();
        // Register the token for /stop. Spawned because `callbacks` is sync;
        // the store is a quick lock and races are benign (a /stop arriving in
        // the microseconds before registration simply finds no token, same as
        // today when it arrives before the agent call starts).
        tokio::spawn(async move {
            state.store_cancel_token(session_id, token).await;
        });

        crate::channels::gateway::surface::SurfaceCallbacks {
            approval: Some(crate::channels::telegram::handler::make_approval_callback(
                self.state.clone(),
            )),
            progress: None,
            question: Some(
                crate::channels::telegram::follow_up_question::make_question_callback(
                    self.state.clone(),
                    None,
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
        use teloxide::prelude::Requester;
        use teloxide::types::InputFile;

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

        // Per-turn delivery context stashed by the listener on publish. Absent
        // only if the listener path didn't set it (shouldn't happen for a real
        // turn) — fall back to safe defaults (treat as DM, no voice).
        let dctx = self.state.take_delivery_context(chat_id).await;
        let is_dm = dctx.as_ref().map(|d| d.is_dm).unwrap_or(true);
        let is_voice = dctx.as_ref().map(|d| d.is_voice).unwrap_or(false);

        // Resolve the forum topic: stashed thread id wins, then explicit
        // thread_key, then the most recent topic seen for this chat (#130).
        let thread_id = match dctx.as_ref().and_then(|d| d.thread_id).or_else(|| {
            target
                .thread_key
                .as_deref()
                .and_then(|s| s.parse::<i32>().ok())
        }) {
            Some(tid) => Some(teloxide::types::ThreadId(teloxide::types::MessageId(tid))),
            None => crate::channels::telegram::send::latest_thread_id_for_chat(chat_id).await,
        };

        // Extract <<IMG:path>> markers handled centrally by the gateway — send
        // each as a Telegram photo.
        for img_path in &message.images {
            match tokio::fs::read(img_path).await {
                Ok(bytes) => {
                    if let Err(e) = crate::channels::telegram::send::photo_in_thread(
                        &bot,
                        ChatId(chat_id),
                        thread_id,
                        InputFile::memory(bytes),
                    )
                    .await
                    {
                        tracing::error!("Telegram: failed to send generated image: {}", e);
                    }
                }
                Err(e) => tracing::error!("Telegram: failed to read image {}: {}", img_path, e),
            }
        }

        // Final text reply: strip artifacts, render HTML, chunk to 4096, send
        // each in-thread. No streaming edit-loop / intermediate dedup — under
        // "simplify replies" the reply is a single delivered message.
        let text_only = crate::utils::sanitize::strip_llm_artifacts(&message.text);
        let text_only = crate::utils::sanitize::redact_secrets(&text_only);
        if !text_only.trim().is_empty() {
            let html = crate::channels::telegram::handler::markdown_to_telegram_html(&text_only);
            for chunk in crate::channels::telegram::handler::split_message(&html, 4096) {
                crate::channels::telegram::handler::send_html_or_plain(
                    &bot,
                    ChatId(chat_id),
                    thread_id,
                    chunk,
                )
                .await
                .map_err(|e| anyhow::anyhow!("Telegram send failed: {e}"))?;
            }
        }

        // Record the bot's reply into channel_messages for group chats so the
        // next group turn's recent() query sees both sides (DMs use the
        // session messages table directly and skip this).
        if !is_dm && !text_only.trim().is_empty() {
            let bot_display_name = self
                .state
                .bot_username()
                .await
                .map(|u| format!("@{}", u))
                .unwrap_or_else(|| "OpenCrabs".to_string());
            let chat_title = dctx
                .as_ref()
                .map(|d| d.chat_title.clone())
                .unwrap_or_default();
            let thread_str = thread_id.map(|t| t.0.0.to_string());
            let cm = crate::db::models::ChannelMessage::new(
                "telegram".to_string(),
                chat_id.to_string(),
                Some(chat_title),
                "bot:opencrabs".to_string(),
                bot_display_name,
                text_only.clone(),
                "text".to_string(),
                None,
            )
            .with_thread(thread_str, None);
            let repo = crate::db::ChannelMessageRepository::new(self.db_pool.clone());
            if let Err(e) = repo.insert(&cm).await {
                tracing::warn!(
                    "Telegram: failed to record bot reply in channel_messages: {}",
                    e
                );
            }
        }

        // Voice-input turns get a synthesized voice reply when TTS is enabled.
        if is_voice
            && let Some(ref d) = dctx
            && d.voice_config.tts_enabled
        {
            match crate::channels::voice::synthesize(&message.text, &d.voice_config).await {
                Ok(audio_bytes) => {
                    if let Err(e) = bot
                        .send_voice(ChatId(chat_id), InputFile::memory(audio_bytes))
                        .await
                    {
                        tracing::error!("Telegram: send_voice failed: {}", e);
                    }
                }
                Err(e) => tracing::error!("Telegram: TTS synthesis failed: {:#}", e),
            }
        }

        // Context-budget footer as a trailing message (metadata about the turn).
        let ctx_max = self.agent.context_limit_for_session(message.session_id);
        let footer = crate::utils::format_ctx_footer(
            message.full.context_tokens,
            ctx_max,
            message.full.tokens_per_second,
        );
        if !footer.trim().is_empty()
            && let Err(e) = crate::channels::telegram::send::message_in_thread(
                &bot,
                ChatId(chat_id),
                thread_id,
                &footer,
            )
            .await
        {
            tracing::warn!("Telegram: failed to send ctx footer: {}", e);
        }

        // The turn is complete — drop the cancel token so a later /stop doesn't
        // hit a stale entry.
        self.state.remove_cancel_token(message.session_id).await;
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
