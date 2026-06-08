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

    async fn start(self: Arc<Self>, bus: GatewayHandle) -> JoinHandle<()> {
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
            bus,
        );
        agent.start(token)
    }

    fn callbacks(
        &self,
        _conversation_key: &str,
        session_id: uuid::Uuid,
    ) -> crate::channels::gateway::surface::SurfaceCallbacks {
        // Rebuild Discord's interactive callbacks from shared state and register
        // a cancel token for /stop. Approval + follow-up-question resolve the
        // channel from `discord_state.session_channel(session_id)`.
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let state = self.state.clone();
        let token = cancel_token.clone();
        tokio::spawn(async move {
            state.store_cancel_token(session_id, token).await;
        });

        crate::channels::gateway::surface::SurfaceCallbacks {
            approval: Some(crate::channels::discord::handler::make_approval_callback(
                self.state.clone(),
            )),
            progress: None,
            question: Some(
                crate::channels::discord::follow_up_question::make_question_callback(
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
        use serenity::builder::{CreateAttachment, CreateMessage};

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

        let dctx = self.state.take_delivery_context(channel_id).await;
        let is_dm = dctx.as_ref().map(|d| d.is_dm).unwrap_or(true);
        let is_voice = dctx.as_ref().map(|d| d.is_voice).unwrap_or(false);

        // Send extracted images as file attachments.
        for img_path in &message.images {
            match tokio::fs::read(img_path).await {
                Ok(bytes) => {
                    let fname = std::path::Path::new(img_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("image.png")
                        .to_string();
                    let file = CreateAttachment::bytes(bytes.as_slice(), fname);
                    if let Err(e) = channel
                        .send_message(&http, CreateMessage::new().add_file(file))
                        .await
                    {
                        tracing::error!("Discord: failed to send generated image: {}", e);
                    }
                }
                Err(e) => tracing::error!("Discord: failed to read image {}: {}", img_path, e),
            }
        }

        // Final text reply (single message, no streaming), chunked to 2000.
        let text_only = crate::utils::sanitize::strip_llm_artifacts(&message.text);
        let text_only = crate::utils::sanitize::redact_secrets(&text_only);
        if !text_only.trim().is_empty() {
            for chunk in crate::channels::discord::handler::split_message(&text_only, 2000) {
                channel
                    .say(&http, chunk)
                    .await
                    .map_err(|e| anyhow::anyhow!("Discord send failed: {e}"))?;
            }
        }

        // Record bot reply for guild context (skip DMs).
        if !is_dm && !text_only.trim().is_empty() {
            let bot_sender_id = self
                .state
                .bot_user_id()
                .await
                .map(|id| id.to_string())
                .unwrap_or_else(|| "bot:opencrabs".to_string());
            let guild_name = dctx
                .as_ref()
                .map(|d| d.guild_name.clone())
                .unwrap_or_else(|| "DM".to_string());
            let cm = crate::db::models::ChannelMessage::new(
                "discord".into(),
                channel_id.to_string(),
                Some(guild_name),
                bot_sender_id,
                "OpenCrabs".into(),
                text_only.clone(),
                "text".into(),
                None,
            );
            let repo = crate::db::ChannelMessageRepository::new(self.db_pool.clone());
            if let Err(e) = repo.insert(&cm).await {
                tracing::warn!(
                    "Discord: failed to record bot reply in channel_messages: {}",
                    e
                );
            }
        }

        // TTS voice reply for voice-input turns.
        if is_voice
            && let Some(ref d) = dctx
            && d.voice_config.tts_enabled
        {
            match crate::channels::voice::synthesize(&message.text, &d.voice_config).await {
                Ok(audio_bytes) => {
                    let file = CreateAttachment::bytes(audio_bytes.as_slice(), "response.ogg");
                    if let Err(e) = channel
                        .send_message(&http, CreateMessage::new().add_file(file))
                        .await
                    {
                        tracing::error!("Discord: failed to send TTS voice: {e}");
                    }
                }
                Err(e) => tracing::error!("Discord: TTS error: {e}"),
            }
        }

        // Context-budget footer.
        let ctx_max = self.agent.context_limit_for_session(message.session_id);
        let footer = crate::utils::format_ctx_footer(
            message.full.context_tokens,
            ctx_max,
            message.full.tokens_per_second,
        );
        if !footer.trim().is_empty()
            && let Err(e) = channel.say(&http, &footer).await
        {
            tracing::warn!("Discord: failed to send ctx footer: {}", e);
        }

        self.state.remove_cancel_token(message.session_id).await;
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
