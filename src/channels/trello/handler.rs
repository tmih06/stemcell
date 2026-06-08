//! Trello Comment Handler
//!
//! Routes incoming card comments onto the gateway bus and (via the Trello
//! surface's `deliver`) posts agent responses back as card comments.

use super::client::TrelloClient;
use super::models::Action;
use crate::channels::gateway::bus::GatewayHandle;
use crate::channels::gateway::envelope::{Inbound, SenderRef};
use crate::services::SessionService;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Process a single Trello card comment: resolve its session and publish it
/// onto the gateway bus. The gateway runs the agent turn and routes the
/// response back through [`TrelloSurface::deliver`], which calls
/// [`post_reply`].
#[allow(clippy::too_many_arguments)]
pub async fn process_comment(
    comment: &Action,
    client: &TrelloClient,
    gateway: &GatewayHandle,
    session_svc: SessionService,
    shared_session: Arc<Mutex<Option<Uuid>>>,
    owner_member_id: Option<&str>,
    idle_timeout_hours: Option<f64>,
) {
    let card_id = match &comment.data.card {
        Some(c) => c.id.clone(),
        None => {
            tracing::warn!("Trello: comment action has no card reference, skipping");
            return;
        }
    };

    let card_name = comment
        .data
        .card
        .as_ref()
        .map(|c| c.name.as_str())
        .unwrap_or("unknown card");

    let commenter_id = &comment.member_creator.id;
    let commenter_name = &comment.member_creator.full_name;
    let text = comment.data.text.trim();

    if text.is_empty() {
        return;
    }

    // Determine whether this commenter is the "owner" (first in allowed_users)
    let is_owner = owner_member_id
        .map(|id| id == commenter_id.as_str())
        .unwrap_or(false);

    // Resolve or create a session for this commenter
    let session_id = if is_owner {
        let shared = shared_session.lock().await;
        match *shared {
            Some(id) => id,
            None => {
                drop(shared);
                tracing::warn!("Trello: no active TUI session, creating one for owner");
                match crate::channels::session_init::create_channel_session(
                    &session_svc,
                    Some("Trello".to_string()),
                )
                .await
                {
                    Ok(s) => {
                        *shared_session.lock().await = Some(s.id);
                        s.id
                    }
                    Err(e) => {
                        tracing::error!("Trello: failed to create owner session: {}", e);
                        return;
                    }
                }
            }
        }
    } else {
        // Non-owner sessions: persisted in DB by title — survives restarts.
        let session_title = format!("Trello: {}", commenter_name);

        let existing = session_svc
            .find_session_by_title(&session_title)
            .await
            .ok()
            .flatten();

        if let Some(session) = existing {
            if idle_timeout_hours.is_some_and(|h| {
                let elapsed = (chrono::Utc::now() - session.updated_at).num_seconds();
                elapsed > (h * 3600.0) as i64
            }) {
                let _ = session_svc.archive_session(session.id).await;
                match crate::channels::session_init::create_channel_session(
                    &session_svc,
                    Some(session_title),
                )
                .await
                {
                    Ok(new_session) => new_session.id,
                    Err(e) => {
                        tracing::error!(
                            "Trello: failed to create session for {}: {}",
                            commenter_name,
                            e
                        );
                        return;
                    }
                }
            } else {
                session.id
            }
        } else {
            match crate::channels::session_init::create_channel_session(
                &session_svc,
                Some(session_title),
            )
            .await
            {
                Ok(session) => {
                    tracing::info!(
                        "Trello: created new session {} for {}",
                        session.id,
                        commenter_name
                    );
                    session.id
                }
                Err(e) => {
                    tracing::error!(
                        "Trello: failed to create session for {}: {}",
                        commenter_name,
                        e
                    );
                    return;
                }
            }
        }
    };

    // Fetch card attachments and include images/text files in context
    let mut attachment_context = String::new();
    if let Ok(attachments) = client.get_card_attachments(&card_id).await {
        use crate::utils::{inject_file_content, process_file_with_vision};
        for att in &attachments {
            let url = match att.url.as_deref() {
                Some(u) if !u.is_empty() => u,
                _ => continue,
            };
            let mime = att.mime_type.as_str();
            let fname = att.name.as_str();

            // Download attachment bytes
            let bytes = match client.download_attachment(url).await {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("Trello: failed to download attachment '{}': {}", fname, e);
                    continue;
                }
            };

            if let Ok(cfg) = crate::config::Config::load() {
                let fc = process_file_with_vision(&bytes, mime, fname, &cfg);
                let injected = inject_file_content(&fc).0;
                if !injected.is_empty() {
                    attachment_context.push_str(&format!("\n\n{injected}"));
                }
            }
        }
    }

    // Build context-enriched message
    let message = if attachment_context.is_empty() {
        format!("[Trello card: {}]\n{}", card_name, text)
    } else {
        format!(
            "[Trello card: {}]\n{}{}",
            card_name, text, attachment_context
        )
    };

    // Display version: clean comment for the TUI session, prefixed with the
    // commenter's name so multi-user Trello cards stay readable.
    let display_text = format!("{commenter_name}: {text}");

    tracing::info!(
        "Trello: comment on '{}' from {} — publishing to gateway (session {})",
        card_name,
        commenter_name,
        session_id
    );

    // Publish onto the bus. The conversation_key is the card id, so the
    // gateway addresses the response back to this card via the surface's
    // `deliver`. The session was resolved here (owner shares the TUI session;
    // non-owners key per-commenter), so it rides along as `session_hint` and
    // the gateway honors it rather than re-resolving from the card id.
    let mut inbound = Inbound::new(
        "trello",
        card_id,
        SenderRef::new(commenter_id.clone(), commenter_name.clone()),
        message,
    );
    inbound.display_text = Some(display_text);
    inbound.session_hint = Some(session_id);

    if !gateway.publish_inbound(inbound) {
        tracing::warn!("Trello: gateway rejected inbound (queue full or closed) for card");
    }
}

/// Post an agent reply back to a Trello card: upload any inline images as card
/// attachments, embed them in markdown, then post the (chunked) comment. Shared
/// by the Trello surface's `deliver`; this is the outbound half the gateway
/// drives after running the agent turn.
pub async fn post_reply(client: &TrelloClient, card_id: &str, text: &str, images: &[String]) {
    // Embed extracted image markers as uploaded card attachments.
    let mut image_embeds: Vec<String> = Vec::new();
    for img_path in images {
        match tokio::fs::read(img_path).await {
            Ok(bytes) => {
                let filename = std::path::Path::new(img_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "image.png".to_string());
                let mime = crate::utils::file_extract::mime_from_ext(&filename);
                match client
                    .add_attachment_to_card(card_id, bytes, &filename, mime)
                    .await
                {
                    Ok(att_url) => {
                        image_embeds.push(format!("![{}]({})", filename, att_url));
                    }
                    Err(e) => {
                        tracing::warn!("Trello: failed to upload image '{}': {}", filename, e);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Trello: failed to read image file '{}': {}", img_path, e);
            }
        }
    }

    let trimmed = text.trim();
    let final_reply = match (trimmed.is_empty(), image_embeds.is_empty()) {
        (true, true) => return,
        (true, false) => image_embeds.join("\n"),
        (false, true) => trimmed.to_string(),
        (false, false) => format!("{}\n\n{}", trimmed, image_embeds.join("\n")),
    };

    // Split at ~4000 chars on newlines (Trello limit is ~16 384 chars per comment,
    // but we keep chunks short so they read well in the card activity feed).
    let chunks = split_comment(&final_reply, 4000);
    for chunk in chunks {
        if let Err(e) = client.add_comment_to_card(card_id, &chunk).await {
            tracing::error!("Trello: failed to post reply on card '{}': {}", card_id, e);
        }
    }
}

/// Split a long comment into chunks of at most `max_len` characters,
/// breaking preferably on newlines.
pub fn split_comment(text: &str, max_len: usize) -> Vec<String> {
    if text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while remaining.len() > max_len {
        // Ensure we split on a char boundary (back up if inside a multi-byte char)
        let mut safe_max = max_len;
        while safe_max > 0 && !remaining.is_char_boundary(safe_max) {
            safe_max -= 1;
        }
        let split_at = match remaining[..safe_max].rfind('\n') {
            Some(pos) => pos + 1,
            None => safe_max,
        };
        chunks.push(remaining[..split_at].to_string());
        remaining = &remaining[split_at..];
    }

    if !remaining.is_empty() {
        chunks.push(remaining.to_string());
    }

    chunks
}
