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

    async fn start(self: Arc<Self>, bus: GatewayHandle) -> JoinHandle<()> {
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
            bus,
        );
        agent.start(bot_token, app_token)
    }

    fn callbacks(
        &self,
        _conversation_key: &str,
        session_id: uuid::Uuid,
    ) -> crate::channels::gateway::surface::SurfaceCallbacks {
        // Rebuild Slack's interactive callbacks from shared state, register a
        // cancel token for /stop. Approval + follow-up-question resolve the
        // channel from `slack_state.session_channel(session_id)`.
        let cancel_token = tokio_util::sync::CancellationToken::new();
        let state = self.state.clone();
        let token = cancel_token.clone();
        tokio::spawn(async move {
            state.store_cancel_token(session_id, token).await;
        });

        crate::channels::gateway::surface::SurfaceCallbacks {
            approval: Some(crate::channels::slack::handler::make_approval_callback(
                self.state.clone(),
            )),
            progress: None,
            question: Some(
                crate::channels::slack::follow_up_question::make_question_callback(
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
        let channel = SlackChannelId::new(target.conversation_key.clone());

        let dctx = self
            .state
            .take_delivery_context(&target.conversation_key)
            .await;
        let is_voice = dctx.as_ref().map(|d| d.is_voice).unwrap_or(false);
        let thread_ts = dctx
            .as_ref()
            .and_then(|d| d.thread_ts.clone())
            .or_else(|| target.thread_key.clone())
            .map(SlackTs::new);

        // Upload extracted images as files into the channel.
        for img_path in &message.images {
            match tokio::fs::read(img_path).await {
                Ok(bytes) => {
                    let fname = std::path::Path::new(img_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("image.png")
                        .to_string();
                    #[allow(deprecated)]
                    let req = SlackApiFilesUploadRequest {
                        channels: Some(vec![channel.clone()]),
                        binary_content: Some(bytes),
                        filename: Some(fname),
                        filetype: None,
                        content: None,
                        initial_comment: None,
                        thread_ts: thread_ts.clone(),
                        title: None,
                        file_content_type: Some("image/png".to_string()),
                    };
                    #[allow(deprecated)]
                    if let Err(e) = session.files_upload(&req).await {
                        tracing::error!("Slack: failed to upload generated image: {}", e);
                    }
                }
                Err(e) => tracing::error!("Slack: failed to read image {}: {}", img_path, e),
            }
        }

        // Final text reply (single message, no streaming).
        let (text_only, _img) = crate::utils::extract_img_markers(&message.text);
        let (text_only, _vid) = crate::utils::extract_vid_markers(&text_only);
        let text_only = crate::utils::sanitize::strip_llm_artifacts(&text_only);
        let text_only = crate::utils::sanitize::redact_secrets(&text_only);
        let mrkdwn = crate::utils::slack_fmt::markdown_to_mrkdwn(&text_only);
        if !mrkdwn.trim().is_empty() {
            let mut req = SlackApiChatPostMessageRequest::new(
                channel.clone(),
                SlackMessageContent::new().with_text(mrkdwn),
            );
            if let Some(ref ts) = thread_ts {
                req = req.with_thread_ts(ts.clone());
            }
            session
                .chat_post_message(&req)
                .await
                .map_err(|e| anyhow::anyhow!("Slack send failed: {e}"))?;
        }

        // Record the bot's reply in channel_messages (all Slack chats, matching
        // the listener's passive capture) so next-turn context sees both sides.
        if !text_only.trim().is_empty() {
            let channel_name = dctx
                .as_ref()
                .map(|d| d.channel_name.clone())
                .unwrap_or_default();
            let cm = crate::db::models::ChannelMessage::new(
                "slack".into(),
                target.conversation_key.clone(),
                Some(channel_name),
                "bot:stemcell".to_string(),
                "StemCell".to_string(),
                text_only.clone(),
                "text".into(),
                None,
            );
            let repo = crate::db::ChannelMessageRepository::new(self.db_pool.clone());
            if let Err(e) = repo.insert(&cm).await {
                tracing::warn!(
                    "Slack: failed to record bot reply in channel_messages: {}",
                    e
                );
            }
        }

        // TTS voice reply (uploaded as an audio file) for voice-input turns.
        if is_voice
            && let Some(ref d) = dctx
            && d.voice_config.tts_enabled
        {
            match crate::channels::voice::synthesize(&message.text, &d.voice_config).await {
                Ok(audio_bytes) => {
                    #[allow(deprecated)]
                    let req = SlackApiFilesUploadRequest {
                        channels: Some(vec![channel.clone()]),
                        binary_content: Some(audio_bytes),
                        filename: Some("response.ogg".to_string()),
                        filetype: None,
                        content: None,
                        initial_comment: None,
                        thread_ts: thread_ts.clone(),
                        title: None,
                        file_content_type: Some("audio/ogg".to_string()),
                    };
                    #[allow(deprecated)]
                    if let Err(e) = session.files_upload(&req).await {
                        tracing::error!("Slack: failed to upload TTS voice: {}", e);
                    }
                }
                Err(e) => tracing::error!("Slack: TTS error: {e}"),
            }
        }

        // Context-budget footer.
        let ctx_max = self.agent.context_limit_for_session(message.session_id);
        let footer = crate::utils::format_ctx_footer(
            message.full.context_tokens,
            ctx_max,
            message.full.tokens_per_second,
        );
        if !footer.trim().is_empty() {
            let mut req = SlackApiChatPostMessageRequest::new(
                channel.clone(),
                SlackMessageContent::new().with_text(footer),
            );
            if let Some(ref ts) = thread_ts {
                req = req.with_thread_ts(ts.clone());
            }
            if let Err(e) = session.chat_post_message(&req).await {
                tracing::warn!("Slack: failed to send ctx footer: {}", e);
            }
        }

        self.state.remove_cancel_token(message.session_id).await;
        Ok(())
    }
}
