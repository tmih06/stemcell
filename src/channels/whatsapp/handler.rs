//! WhatsApp Message Handler
//!
//! Processes incoming WhatsApp messages: text + images, allowlist enforcement,
//! session routing (owner shares TUI session, others get per-phone sessions).

use crate::brain::agent::AgentService;
use crate::brain::agent::ApprovalCallback;
use crate::channels::whatsapp::WhatsAppState;
use crate::config::Config;
use crate::db::ChannelMessageRepository;
use crate::db::models::ChannelMessage as DbChannelMessage;
use crate::services::SessionService;
use crate::utils::sanitize::redact_secrets;
use crate::utils::truncate_str;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use uuid::Uuid;

use wacore::types::message::MessageInfo;
use waproto::whatsapp::Message;
use whatsapp_rust::client::Client;

/// Header prepended to all outgoing messages so the user knows it's from the agent.
pub const MSG_HEADER: &str = "\u{1f980} *OpenCrabs*";

/// Unwrap nested message wrappers (device_sent, ephemeral, view_once, etc.)
/// Returns the innermost Message that contains actual content.
fn unwrap_message(msg: &Message) -> &Message {
    // device_sent_message: wraps messages synced across linked devices
    if let Some(ref dsm) = msg.device_sent_message
        && let Some(ref inner) = dsm.message
    {
        return unwrap_message(inner);
    }
    // ephemeral_message: disappearing messages
    if let Some(ref eph) = msg.ephemeral_message
        && let Some(ref inner) = eph.message
    {
        return unwrap_message(inner);
    }
    // view_once_message
    if let Some(ref vo) = msg.view_once_message
        && let Some(ref inner) = vo.message
    {
        return unwrap_message(inner);
    }
    // document_with_caption_message
    if let Some(ref dwc) = msg.document_with_caption_message
        && let Some(ref inner) = dwc.message
    {
        return unwrap_message(inner);
    }
    msg
}

/// Extract quoted/replied-to message text from a WhatsApp message.
fn extract_reply_context(msg: &Message) -> Option<String> {
    let msg = unwrap_message(msg);
    let ctx = msg.extended_text_message.as_ref()?.context_info.as_ref()?;
    let quoted = ctx.quoted_message.as_ref()?;
    let quoted_text = extract_text(quoted)?;
    if quoted_text.is_empty() {
        return None;
    }
    let sender = ctx
        .participant
        .as_ref()
        .map(|p| p.split('@').next().unwrap_or(p).to_string())
        .unwrap_or_else(|| "unknown".to_string());
    Some(format!("[Replying to {sender}: \"{quoted_text}\"]"))
}

/// Extract plain text from a WhatsApp message.
pub(crate) fn extract_text(msg: &Message) -> Option<String> {
    let msg = unwrap_message(msg);
    // Try conversation field first (simple text messages)
    if let Some(ref conv) = msg.conversation
        && !conv.is_empty()
    {
        return Some(conv.clone());
    }
    // Try extended text message (messages with link previews, etc.)
    if let Some(ref ext) = msg.extended_text_message
        && let Some(ref text) = ext.text
    {
        return Some(text.clone());
    }
    // Try image caption
    if let Some(ref img) = msg.image_message
        && let Some(ref caption) = img.caption
        && !caption.is_empty()
    {
        return Some(caption.clone());
    }
    None
}

/// Check if the message has a downloadable image.
pub(crate) fn has_image(msg: &Message) -> bool {
    let msg = unwrap_message(msg);
    msg.image_message.is_some()
}

/// Check if the message has a downloadable audio/voice note.
fn has_audio(msg: &Message) -> bool {
    let msg = unwrap_message(msg);
    msg.audio_message.is_some()
}

/// Check if the message has a document attachment.
fn has_document(msg: &Message) -> bool {
    let msg = unwrap_message(msg);
    msg.document_message.is_some()
}

/// Download a document from WhatsApp. Returns (bytes, mime, filename) on success.
async fn download_document(msg: &Message, client: &Client) -> Option<(Vec<u8>, String, String)> {
    let msg = unwrap_message(msg);
    let doc = msg.document_message.as_ref()?;
    let mime = doc.mimetype.clone().unwrap_or_default();
    let fname = doc.file_name.clone().unwrap_or_else(|| "file".to_string());
    match client.download(doc.as_ref()).await {
        Ok(bytes) => {
            tracing::debug!(
                "WhatsApp: downloaded document {} ({} bytes)",
                fname,
                bytes.len()
            );
            Some((bytes, mime, fname))
        }
        Err(e) => {
            tracing::error!("WhatsApp: failed to download document: {e}");
            None
        }
    }
}

/// Download audio from WhatsApp. Returns raw bytes on success.
async fn download_audio(msg: &Message, client: &Client) -> Option<Vec<u8>> {
    let msg = unwrap_message(msg);
    let audio = msg.audio_message.as_ref()?;
    match client.download(audio.as_ref()).await {
        Ok(bytes) => {
            tracing::debug!("WhatsApp: downloaded audio ({} bytes)", bytes.len());
            Some(bytes)
        }
        Err(e) => {
            tracing::error!("WhatsApp: failed to download audio: {e}");
            None
        }
    }
}

/// Download image from WhatsApp. Returns (bytes, mime, filename) on success.
async fn download_image(msg: &Message, client: &Client) -> Option<(Vec<u8>, String, String)> {
    let msg = unwrap_message(msg);
    let img = msg.image_message.as_ref()?;

    let mime = img.mimetype.as_deref().unwrap_or("image/jpeg").to_string();
    let ext = match mime.as_str() {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "jpg",
    };
    let fname = format!("image.{ext}");

    match client.download(img.as_ref()).await {
        Ok(bytes) => {
            tracing::debug!(
                "WhatsApp: downloaded image ({} bytes, mime={})",
                bytes.len(),
                mime
            );
            Some((bytes, mime, fname))
        }
        Err(e) => {
            tracing::error!("WhatsApp: failed to download image: {e}");
            None
        }
    }
}

/// Extract the sender's phone number (digits only) from message info.
/// JID format is "351933536442@s.whatsapp.net" or "351933536442:34@s.whatsapp.net"
/// Extract sender phone from MessageInfo.
/// (linked device suffix) — we return just "351933536442" in both cases.
fn sender_phone(info: &MessageInfo) -> String {
    let full = info.source.sender.to_string();
    let without_server = full.split('@').next().unwrap_or(&full);
    // Strip linked-device suffix (e.g. ":34" for WhatsApp Web/Desktop)
    without_server
        .split(':')
        .next()
        .unwrap_or(without_server)
        .to_string()
}

/// Extract recipient phone from MessageInfo (who the message is TO).
fn recipient_phone(info: &MessageInfo) -> Option<String> {
    info.source.recipient.as_ref().map(|r| {
        let full = r.to_string();
        let without_server = full.split('@').next().unwrap_or(&full);
        without_server
            .split(':')
            .next()
            .unwrap_or(without_server)
            .to_string()
    })
}

/// Split a message into chunks that fit WhatsApp's limit (~65536 chars, but we use 4000 for readability).
pub fn split_message(text: &str, max_len: usize) -> Vec<&str> {
    if text.len() <= max_len {
        return vec![text];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + max_len).min(text.len());
        // Ensure end falls on a char boundary (back up if inside a multi-byte char)
        while end < text.len() && !text.is_char_boundary(end) {
            end -= 1;
        }
        let break_at = if end < text.len() {
            text[start..end]
                .rfind('\n')
                .filter(|&pos| pos > end - start - 200)
                .map(|pos| start + pos + 1)
                .unwrap_or(end)
        } else {
            end
        };
        chunks.push(&text[start..break_at]);
        start = break_at;
    }
    chunks
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_message(
    msg: Message,
    info: MessageInfo,
    client: Arc<Client>,
    agent: Arc<AgentService>,
    session_svc: SessionService,
    shared_session: Arc<TokioMutex<Option<Uuid>>>,
    wa_state: Arc<WhatsAppState>,
    config_rx: tokio::sync::watch::Receiver<Config>,
    channel_msg_repo: ChannelMessageRepository,
    gateway: crate::channels::gateway::bus::GatewayHandle,
) {
    let phone = sender_phone(&info);
    tracing::debug!(
        "WhatsApp handler: from={}, is_from_me={}, has_text={}, has_image={}, has_audio={}",
        phone,
        info.source.is_from_me,
        extract_text(&msg).is_some(),
        has_image(&msg),
        has_audio(&msg),
    );

    // Skip bot's own outgoing replies (they echo back as is_from_me).
    // User messages from their phone are also is_from_me (same account),
    // so we only skip if the text starts with our agent header.
    // Never skip audio/image — those are real user messages even when is_from_me.
    if info.source.is_from_me {
        if let Some(text) = extract_text(&msg) {
            if text.starts_with(MSG_HEADER) {
                return;
            }
        } else if !has_audio(&msg) && !has_image(&msg) {
            // No text, no audio, no image and is_from_me — non-content echo, skip
            return;
        }
    }

    // Build message content: text, image, audio, or document
    let has_img = has_image(&msg);
    let has_aud = has_audio(&msg);
    let has_doc = has_document(&msg);
    let text = extract_text(&msg);

    // Require at least text, image, audio, or document
    if text.is_none() && !has_img && !has_aud && !has_doc {
        return;
    }

    // Passively capture message for channel history (groups and DMs)
    if let Some(ref t) = text
        && !t.is_empty()
    {
        let chat_id = format!("{}", info.source.chat);
        let is_group = info.source.is_group;
        let push_name = info.push_name.clone();
        let cm = DbChannelMessage::new(
            "whatsapp".into(),
            chat_id,
            if is_group {
                Some(format!("{}", info.source.chat))
            } else {
                None
            },
            phone.clone(),
            push_name,
            t.clone(),
            "text".into(),
            None,
        );
        if let Err(e) = channel_msg_repo.insert(&cm).await {
            tracing::warn!("Failed to store WhatsApp channel message: {e}");
        }
    }

    // Read latest config from watch channel — single source of truth
    let cfg = config_rx.borrow().clone();
    let wa_cfg = &cfg.channels.whatsapp;
    let allowed: HashSet<String> = wa_cfg.allowed_phones.iter().cloned().collect();
    let idle_timeout_hours = wa_cfg.session_idle_hours;
    let voice_config = cfg.voice_config();

    // SECURITY: When allowed_phones is configured, only respond to the owner.
    // Also check the recipient: when owner sends a message TO a contact,
    // sender=owner but recipient=contact — must not treat that as "owner messaging bot".
    // If allowed_phones is empty (unconfigured), fall through without filtering.
    if !allowed.is_empty() {
        let owner_phone_raw = allowed.iter().next().cloned().unwrap_or_default();
        let owner_phone = owner_phone_raw.trim_start_matches('+');
        let sender_normalized = phone.trim_start_matches('+');
        let recipient = recipient_phone(&info);
        let recipient_normalized = recipient.as_ref().map(|r| r.trim_start_matches('+'));
        let is_to_owner = recipient_normalized
            .map(|r| r == owner_phone)
            .unwrap_or(false);
        let is_from_owner = sender_normalized == owner_phone;
        if !is_from_owner || (recipient.is_some() && !is_to_owner) {
            tracing::debug!(
                "WhatsApp: ignoring message from={} to={:?} (owner={})",
                phone,
                recipient,
                owner_phone
            );
            return;
        }
    }

    // Pending approval check: if a tool approval is waiting for this phone,
    // interpret this message as Yes / Always / No instead of routing to the agent.
    // Handles both button taps (ButtonsResponseMessage) and plain text replies.
    {
        use crate::channels::whatsapp::WaApproval;

        let btn_id = unwrap_message(&msg)
            .buttons_response_message
            .as_ref()
            .and_then(|b| b.selected_button_id.as_deref());

        let choice: Option<WaApproval> = if let Some(id) = btn_id {
            match id {
                "wa_approve_yes" => Some(WaApproval::Yes),
                "wa_approve_always" => Some(WaApproval::Always),
                "wa_approve_yolo" => Some(WaApproval::Yolo),
                "wa_approve_no" => Some(WaApproval::No),
                _ => None,
            }
        } else if let Some(raw_text) = extract_text(&msg) {
            let answer = raw_text.trim().to_lowercase();
            if matches!(answer.as_str(), "yes" | "y" | "sim" | "s") {
                Some(WaApproval::Yes)
            } else if matches!(answer.as_str(), "always" | "sempre") {
                Some(WaApproval::Always)
            } else if matches!(answer.as_str(), "yolo") {
                Some(WaApproval::Yolo)
            } else if matches!(answer.as_str(), "no" | "n" | "nao" | "não") {
                Some(WaApproval::No)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(c) = choice
            && wa_state.resolve_pending_approval(&phone, c).await.is_some()
        {
            tracing::info!("WhatsApp: approval from {}: {:?}", phone, c);
            if c == WaApproval::Always {
                crate::utils::persist_auto_session_policy();
            } else if c == WaApproval::Yolo {
                crate::utils::persist_auto_always_policy();
            }
            return;
        }

        // Follow-up question intercept: if this phone has a pending
        // question and the incoming text parses as a 1-based option
        // number, resolve the question instead of forwarding to the
        // agent. Any other reply falls through (user can "abandon" a
        // question by typing something unrelated).
        if wa_state.has_pending_question(&phone).await
            && let Some(raw_text) = extract_text(&msg)
            && let Some(answer) = wa_state
                .resolve_pending_question(&phone, raw_text.trim())
                .await
        {
            tracing::info!(
                "WhatsApp follow_up_question resolved from {}: {}",
                phone,
                answer
            );
            return;
        }
    }

    let text_preview = text
        .as_deref()
        .map(|t| truncate_str(t, 50))
        .unwrap_or("[image]");
    tracing::info!("WhatsApp: message from {}: {}", phone, text_preview);

    // Audio/voice note → show typing immediately and transcribe
    if has_aud && voice_config.stt_enabled {
        let _ = client.chatstate().send_composing(&info.source.chat).await;
    }
    let mut content;
    if has_aud
        && voice_config.stt_enabled
        && let Some(audio_bytes) = download_audio(&msg, &client).await
    {
        match crate::channels::voice::transcribe(audio_bytes, &voice_config).await {
            Ok(transcript) => {
                tracing::info!(
                    "WhatsApp: transcribed voice: {}",
                    truncate_str(&transcript, 80)
                );
                content = transcript;
            }
            Err(e) => {
                tracing::error!("WhatsApp: STT error: {e}");
                content = text.unwrap_or_default();
            }
        }
    } else {
        content = text.unwrap_or_default();
    }

    // Download image if present, use photo batching for multi-image support
    if has_img
        && !has_aud
        && let Some((img_bytes, img_mime, img_fname)) = download_image(&msg, &client).await
    {
        use crate::utils::{inject_file_content, process_file_with_vision};
        let cfg = crate::config::Config::load();
        if let Ok(cfg) = cfg {
            let fc = process_file_with_vision(&img_bytes, &img_mime, &img_fname, &cfg);
            let (injected, _) = inject_file_content(&fc);
            if !injected.is_empty() {
                // Buffer the image marker for batching
                let caption = extract_text(&msg);
                wa_state.buffer_photo(&phone, injected, caption).await;

                // Reset debounce timer
                let token = wa_state.reset_photo_debounce(&phone).await;

                // Wait for debounce to expire
                if !wa_state.wait_photo_debounce(&token).await {
                    // Cancelled by another incoming photo, return early
                    return;
                }

                // Debounce expired, drain all buffered photos
                let (markers, first_caption) = wa_state.drain_photo_buffer(&phone).await;
                wa_state.cleanup_photo_debounce(&phone).await;

                if markers.is_empty() {
                    return;
                }

                // Combine all image markers
                content = markers.join("\n\n");

                // Prepend caption if present
                if let Some(caption) = first_caption
                    && !caption.trim().is_empty()
                {
                    content = format!("{}\n\n{}", caption.trim(), content);
                }
            }
        }
    }

    // Handle document attachment
    if has_doc
        && !has_aud
        && !has_img
        && let Some((bytes, mime, fname)) = download_document(&msg, &client).await
    {
        use crate::utils::{inject_file_content, process_file_with_vision};
        let cfg = crate::config::Config::load();
        if let Ok(cfg) = cfg {
            let fc = process_file_with_vision(&bytes, &mime, &fname, &cfg);
            let injected = inject_file_content(&fc).0;
            if !injected.is_empty() {
                content.push_str(&format!("\n\n{injected}"));
            }
        }
    }

    if content.is_empty() {
        return;
    }

    // is_owner still used below for /new / owner-only flows, but session
    // resolution no longer depends on it — every phone gets its own session.
    let is_owner = allowed.is_empty()
        || allowed
            .iter()
            .next()
            .map(|a| a.trim_start_matches('+') == phone)
            .unwrap_or(false);

    // Sessions are ALWAYS isolated per phone — owner no longer shares the
    // TUI session. Each phone gets its own session in the DB. Title carries a
    // stable `[chat:wa-<phone>]` suffix so auto-rename of the visible label
    // still resolves to the same row (issue #121, Discord/Slack/WhatsApp port
    // of the Telegram fix in PR #123).
    let session_id = {
        use crate::channels::session_resolve;
        let legacy_title = format!("WhatsApp: {}", phone);
        let suffix = session_resolve::chat_id_suffix(&format!("wa-{phone}"));
        let session_title = format!("{legacy_title} {suffix}");

        match session_resolve::resolve_or_create_channel_session(
            &session_svc,
            &suffix,
            &legacy_title,
            &session_title,
            idle_timeout_hours,
            "WhatsApp",
        )
        .await
        {
            Ok(id) => id,
            Err(e) => {
                tracing::error!("WhatsApp: failed to resolve session: {}", e);
                return;
            }
        }
    };

    // Follow-up interrupt: cancel any in-flight agent for this session
    wa_state.cancel_session(session_id).await;

    // Restore session's own provider (each session keeps its provider independently)
    let session_meta = session_svc.get_session(session_id).await.ok().flatten();
    crate::channels::commands::sync_provider_for_session(
        &agent,
        session_id,
        session_meta
            .as_ref()
            .and_then(|s| s.provider_name.as_deref()),
        session_meta.as_ref().and_then(|s| s.model.as_deref()),
    )
    .await;

    // ── Channel commands (/help, /usage, /models, /stop) ────────────────────
    {
        use crate::channels::commands::{self, ChannelCommand};
        let cmd = commands::handle_command(&content, session_id, &agent, &session_svc).await;

        // Handle simple text-response commands (Help, Usage, Evolve, Doctor, etc.)
        if let Some(reply_text) = commands::try_execute_text_command(&cmd).await {
            let reply = waproto::whatsapp::Message {
                conversation: Some(reply_text),
                ..Default::default()
            };
            let _ = client.send_message(info.source.chat.clone(), reply).await;
            return;
        }

        match cmd {
            ChannelCommand::Models(resp) => {
                // WhatsApp has no inline buttons — send plain text list
                let reply = waproto::whatsapp::Message {
                    conversation: Some(resp.text),
                    ..Default::default()
                };
                let _ = client.send_message(info.source.chat.clone(), reply).await;
                return;
            }
            ChannelCommand::NewSession => {
                let session_title = format!("WhatsApp: {}", phone);
                // Archive the previous session on /new, except for the owner —
                // owner sessions stay non-archived so they remain visible in
                // /sessions for history review. Guest sessions get archived
                // so the next title lookup resolves cleanly to the new row.
                if !is_owner
                    && let Ok(Some(old)) = session_svc.find_session_by_title(&session_title).await
                    && let Err(e) = session_svc.archive_session(old.id).await
                {
                    tracing::error!("WhatsApp: failed to archive old session {}: {}", old.id, e);
                }
                match crate::channels::session_init::create_channel_session(
                    &session_svc,
                    Some(session_title),
                )
                .await
                {
                    Ok(new_session) => {
                        if is_owner {
                            *shared_session.lock().await = Some(new_session.id);
                        }
                        // Sync provider for the new session so baseline is accurate
                        let new_meta = session_svc.get_session(new_session.id).await.ok().flatten();
                        crate::channels::commands::sync_provider_for_session(
                            &agent,
                            new_session.id,
                            new_meta.as_ref().and_then(|s| s.provider_name.as_deref()),
                            new_meta.as_ref().and_then(|s| s.model.as_deref()),
                        )
                        .await;
                        let baseline = agent.base_context_tokens();
                        let ctx_max = agent.context_limit_for_session(new_session.id);
                        let footer = crate::utils::format_ctx_footer(baseline, ctx_max, None);
                        let msg_text = format!("✅ New session started.\n\n{footer}");
                        let reply = waproto::whatsapp::Message {
                            conversation: Some(msg_text),
                            ..Default::default()
                        };
                        let _ = client.send_message(info.source.chat.clone(), reply).await;
                        tracing::info!(
                            "WhatsApp /new: sent ctx footer='{}' (baseline={}, ctx_max={})",
                            footer,
                            baseline,
                            ctx_max,
                        );
                    }
                    Err(e) => {
                        tracing::error!("WhatsApp: failed to create session: {}", e);
                        let reply = waproto::whatsapp::Message {
                            conversation: Some("Failed to create session.".to_string()),
                            ..Default::default()
                        };
                        let _ = client.send_message(info.source.chat.clone(), reply).await;
                    }
                }
                return;
            }
            ChannelCommand::Sessions(resp) => {
                // WhatsApp has no inline buttons — send plain text list
                let reply = waproto::whatsapp::Message {
                    conversation: Some(resp.text),
                    ..Default::default()
                };
                let _ = client.send_message(info.source.chat.clone(), reply).await;
                return;
            }
            ChannelCommand::Stop => {
                let cancelled = wa_state.cancel_session(session_id).await;
                let text = if cancelled {
                    "Operation cancelled."
                } else {
                    "No operation in progress."
                };
                let reply = waproto::whatsapp::Message {
                    conversation: Some(text.to_string()),
                    ..Default::default()
                };
                let _ = client.send_message(info.source.chat.clone(), reply).await;
                return;
            }
            ChannelCommand::Compact => {
                let status = waproto::whatsapp::Message {
                    conversation: Some("⏳ Compacting context...".to_string()),
                    ..Default::default()
                };
                let _ = client.send_message(info.source.chat.clone(), status).await;
                content =
                    "[SYSTEM: Compact context now. Summarize this conversation for continuity.]"
                        .to_string();
            }
            ChannelCommand::UserPrompt(prompt) => {
                content = prompt;
                // fall through to agent with the prompt as the message
            }
            ChannelCommand::NotACommand => {}
            // Help, Usage, Evolve, Doctor, UserSystem handled by try_execute_text_command above
            _ => {}
        }
    }

    // Extract replied-to message context so the agent knows what the user is referencing.
    let reply_context = extract_reply_context(&msg);

    // Build the human-readable display text (used for DB persistence + TUI).
    // Owner DMs keep the bare text; non-owner / group messages prefix with
    // sender so OpenCrabs sessions stay readable without the LLM-only
    // metadata brackets.
    let display_text = if is_owner && !info.source.is_group {
        content.clone()
    } else {
        let name = info.push_name.trim();
        let sender = if name.is_empty() {
            format!("+{}", phone)
        } else {
            name.to_string()
        };
        format!("{sender}: {content}")
    };

    // For non-owner contacts, prepend sender identity so the agent knows who
    // it's talking to and doesn't assume it's the owner messaging themselves.
    let agent_input = if !is_owner {
        let name = info.push_name.trim().to_string();
        let from = if name.is_empty() {
            format!("+{}", phone)
        } else {
            format!("{} (+{})", name, phone)
        };
        if info.source.is_group {
            let group = info.source.chat.to_string();
            let group_id = group.split('@').next().unwrap_or(&group);
            format!(
                "[WhatsApp group message from {} in group {}]\n{}",
                from, group_id, content
            )
        } else {
            format!("[WhatsApp message from {}]\n{}", from, content)
        }
    } else {
        content
    };

    // Prepend reply context if the user is replying to a specific message.
    let agent_input = if let Some(ref ctx) = reply_context {
        format!("{ctx}\n{agent_input}")
    } else {
        agent_input
    };

    // Inject recent group history so the agent has full conversation context.
    let agent_input = if info.source.is_group {
        let chat_id_str = info.source.chat.to_string();
        match channel_msg_repo
            .recent(Some("whatsapp"), &chat_id_str, 30)
            .await
        {
            Ok(messages) if !messages.is_empty() => {
                let history: Vec<String> = messages
                    .iter()
                    .rev()
                    .map(|m| {
                        let ts = m.created_at.format("%H:%M");
                        format!("[{}] {}: {}", ts, m.sender_name, m.content)
                    })
                    .collect();
                format!(
                    "[Recent group history ({} messages):\n{}\n--- end history ---]\n{}",
                    history.len(),
                    history.join("\n"),
                    agent_input
                )
            }
            _ => agent_input,
        }
    } else {
        agent_input
    };

    // Tell the LLM its text response is automatically delivered to the chat.
    let agent_input = format!(
        "[Channel: WhatsApp — your text response is automatically sent to this chat. \
         There is no whatsapp_send tool. Just reply with text.]\n{agent_input}"
    );

    // ── Publish onto the gateway bus ───────────────────────────────────────────
    // The gateway runs the agent turn and routes the response back through
    // `WhatsAppSurface::deliver`. The session was resolved here, so it rides
    // along as `session_hint`. The phone allowlist gate already passed above
    // (WhatsApp isn't covered by the shared allowlist), so publishing means the
    // surface authorized this turn. Stash per-turn context the surface's
    // callbacks (phone-keyed approval/question) and `deliver` (group-record +
    // TTS) need but the generic envelope doesn't carry.
    let wa_chat_id = format!("{}", info.source.chat);
    wa_state
        .set_delivery_context(
            session_id,
            crate::channels::whatsapp::WhatsAppDeliveryContext {
                phone: phone.clone(),
                chat_jid: info.source.chat.clone(),
                is_group: info.source.is_group,
                is_voice: has_aud,
                voice_config: voice_config.clone(),
            },
        )
        .await;

    let mut inbound = crate::channels::gateway::envelope::Inbound::new(
        "whatsapp",
        wa_chat_id.clone(),
        crate::channels::gateway::envelope::SenderRef::new(phone.clone(), info.push_name.clone()),
        agent_input,
    );
    inbound.display_text = Some(display_text);
    inbound.session_hint = Some(session_id);
    inbound.routing = crate::channels::gateway::envelope::Routing {
        is_direct: !info.source.is_group,
        is_mention: false,
    };

    if !gateway.publish_inbound(inbound) {
        tracing::warn!(
            "WhatsApp: gateway rejected inbound (queue full or closed) for chat {}",
            wa_chat_id
        );
    }
}

/// Deliver an agent reply back to a WhatsApp chat: send extracted images, the
/// text reply (chunked), record the bot reply for context, optionally a TTS
/// voice note, then the context-budget footer. Called by the surface's
/// `deliver` after the gateway runs the turn. Replaces the old inline delivery
/// block; the live progress-streaming is gone ("simplify replies"), but the
/// non-streaming concerns (group-record + TTS) are preserved.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn deliver_reply(
    client: &Arc<Client>,
    chat_jid: wacore_binary::jid::Jid,
    full_text: &str,
    is_group: bool,
    is_voice: bool,
    voice_config: Option<&crate::config::VoiceConfig>,
    ctx_max: u32,
    response: &crate::brain::agent::AgentResponse,
    channel_msg_repo: &ChannelMessageRepository,
) {
    // Extract <<IMG:path>> markers — send each as a real WhatsApp image message.
    let (text_content, img_paths) = crate::utils::extract_img_markers(full_text);
    let text_content = crate::utils::sanitize::strip_llm_artifacts(&text_content);
    let text_content = redact_secrets(&text_content);
    let text_content = crate::utils::slack_fmt::markdown_to_mrkdwn(&text_content);

    for img_path in img_paths {
        match tokio::fs::read(&img_path).await {
            Ok(bytes) => {
                use wacore::download::MediaType;
                use waproto::whatsapp::message::ImageMessage;
                use whatsapp_rust::upload::UploadOptions;
                match client
                    .upload(bytes, MediaType::Image, UploadOptions::default())
                    .await
                {
                    Ok(upload) => {
                        let mime = if img_path.ends_with(".png") {
                            "image/png"
                        } else {
                            "image/jpeg"
                        };
                        let img_msg = waproto::whatsapp::Message {
                            image_message: Some(Box::new(ImageMessage {
                                url: Some(upload.url),
                                direct_path: Some(upload.direct_path),
                                media_key: Some(upload.media_key.to_vec()),
                                file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
                                file_sha256: Some(upload.file_sha256.to_vec()),
                                file_length: Some(upload.file_length),
                                mimetype: Some(mime.to_string()),
                                ..Default::default()
                            })),
                            ..Default::default()
                        };
                        if let Err(e) = client.send_message(chat_jid.clone(), img_msg).await {
                            tracing::error!("WhatsApp: failed to send generated image: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("WhatsApp: image upload failed for {}: {}", img_path, e)
                    }
                }
            }
            Err(e) => tracing::error!("WhatsApp: failed to read image {}: {}", img_path, e),
        }
    }

    // Send text reply (single message, chunked; no progressive streaming).
    if !text_content.is_empty() {
        let tagged = format!("{}\n\n{}", MSG_HEADER, text_content);
        for chunk in split_message(&tagged, 4000) {
            let reply_msg = waproto::whatsapp::Message {
                conversation: Some(chunk.to_string()),
                ..Default::default()
            };
            if let Err(e) = client.send_message(chat_jid.clone(), reply_msg).await {
                tracing::error!("WhatsApp: failed to send reply: {}", e);
            }
        }
    }

    // Record bot reply into channel_messages so next-turn context sees both
    // sides (applies to groups + DMs, matching the listener's capture).
    if !text_content.trim().is_empty() {
        let chat_id = format!("{}", chat_jid);
        let cm = DbChannelMessage::new(
            "whatsapp".into(),
            chat_id.clone(),
            if is_group { Some(chat_id) } else { None },
            "bot:opencrabs".to_string(),
            "OpenCrabs".to_string(),
            text_content.clone(),
            "text".into(),
            None,
        );
        if let Err(e) = channel_msg_repo.insert(&cm).await {
            tracing::warn!(
                "WhatsApp: failed to record bot reply in channel_messages: {}",
                e
            );
        }
    }

    // TTS voice note for voice-input turns.
    if is_voice
        && let Some(vc) = voice_config
        && vc.tts_enabled
    {
        match crate::channels::voice::synthesize(&response.content, vc).await {
            Ok(audio_bytes) => {
                use wacore::download::MediaType;
                use waproto::whatsapp::message::AudioMessage;
                use whatsapp_rust::upload::UploadOptions;
                match client
                    .upload(audio_bytes, MediaType::Audio, UploadOptions::default())
                    .await
                {
                    Ok(upload) => {
                        let audio_msg = waproto::whatsapp::Message {
                            audio_message: Some(Box::new(AudioMessage {
                                url: Some(upload.url),
                                direct_path: Some(upload.direct_path),
                                media_key: Some(upload.media_key.to_vec()),
                                file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
                                file_sha256: Some(upload.file_sha256.to_vec()),
                                file_length: Some(upload.file_length),
                                mimetype: Some("audio/ogg; codecs=opus".to_string()),
                                ptt: Some(true),
                                ..Default::default()
                            })),
                            ..Default::default()
                        };
                        if let Err(e) = client.send_message(chat_jid.clone(), audio_msg).await {
                            tracing::error!("WhatsApp: failed to send TTS voice: {}", e);
                        }
                    }
                    Err(e) => tracing::error!("WhatsApp: TTS upload failed: {}", e),
                }
            }
            Err(e) => tracing::error!("WhatsApp: TTS synthesis failed: {:#}", e),
        }
    }

    // Context-budget footer.
    let footer = crate::utils::format_ctx_footer(
        response.context_tokens,
        ctx_max,
        response.tokens_per_second,
    );
    if !footer.trim().is_empty() {
        let footer_msg = waproto::whatsapp::Message {
            conversation: Some(footer),
            ..Default::default()
        };
        if let Err(e) = client.send_message(chat_jid, footer_msg).await {
            tracing::warn!("WhatsApp: failed to send ctx footer: {}", e);
        }
    }
}

/// Build the surface-side tool-approval callback. Resolves the sender phone +
/// chat JID + client from the per-chat delivery context the listener stashed
/// (the generic `Surface::callbacks` signature can't carry them). Preserves the
/// 3-choice (yes / always / yolo) text approval flow — dropping it would
/// silently auto-approve every tool call on WhatsApp.
pub(crate) fn make_surface_approval_callback(state: Arc<WhatsAppState>) -> ApprovalCallback {
    use crate::channels::whatsapp::WaApproval;
    use crate::utils::{check_approval_policy, persist_auto_session_policy};

    Arc::new(move |tool_info| {
        let state = state.clone();
        Box::pin(async move {
            if let Some(result) = check_approval_policy() {
                return Ok(result);
            }
            let (Some(client), Some(ctx)) = (
                state.client().await,
                // The chat JID is the conversation key; the context is keyed by
                // it. We resolve via the session→? — but callbacks only know
                // session_id. Find the one stashed context whose session is
                // active by scanning is unnecessary: the approval fires inside a
                // single in-flight turn, so look it up by the session's chat.
                state
                    .delivery_context_for_session(tool_info.session_id)
                    .await,
            ) else {
                tracing::warn!("WhatsApp approval: no client/context — denying");
                return Ok((false, false));
            };
            let phone = ctx.phone.clone();
            let chat_jid = ctx.chat_jid.clone();

            let safe_input = crate::utils::redact_tool_input(&tool_info.tool_input);
            let input_preview = serde_json::to_string_pretty(&safe_input).unwrap_or_default();
            let body = format!(
                "🔐 *Tool Approval Required*\n\nTool: `{}`\n```\n{}\n```",
                tool_info.tool_name,
                truncate_str(&input_preview, 600),
            );
            let text_msg = waproto::whatsapp::Message {
                conversation: Some(format!(
                    "{}\n\n{}\n\nReply *yes*, *always* (session), *yolo* (permanent), or *no* (5 min timeout).",
                    MSG_HEADER, body
                )),
                ..Default::default()
            };
            if let Err(e) = client.send_message(chat_jid.clone(), text_msg).await {
                tracing::error!("WhatsApp: failed to send approval request: {}", e);
                return Ok((false, false));
            }

            let (tx, rx) = tokio::sync::oneshot::channel::<WaApproval>();
            state.register_pending_approval(phone.clone(), tx).await;

            match tokio::time::timeout(std::time::Duration::from_secs(300), rx).await {
                Ok(Ok(WaApproval::Yes)) => Ok((true, false)),
                Ok(Ok(WaApproval::Always)) => {
                    persist_auto_session_policy();
                    Ok((true, true))
                }
                Ok(Ok(WaApproval::Yolo)) => {
                    crate::utils::persist_auto_always_policy();
                    Ok((true, true))
                }
                Ok(Ok(WaApproval::No)) => Ok((false, false)),
                _ => {
                    let timeout_msg = waproto::whatsapp::Message {
                        conversation: Some(format!(
                            "{}\n\n⏰ No response in 5 minutes — *{}* was denied.\n\nSend your message again and reply *yes*, *always*, or *no* when prompted.",
                            MSG_HEADER, tool_info.tool_name,
                        )),
                        ..Default::default()
                    };
                    let _ = client.send_message(chat_jid, timeout_msg).await;
                    Ok((false, false))
                }
            }
        })
    })
}
