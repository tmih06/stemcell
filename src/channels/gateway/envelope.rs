//! Normalized message envelopes that flow through the channel gateway bus.
//!
//! Every surface (TUI, Telegram, Discord, …) translates its native message
//! representation into an [`Inbound`] on receipt, and consumes an [`Outbound`]
//! when delivering an agent response. The agent core never sees these types —
//! they exist purely to give the gateway a surface-agnostic vocabulary for
//! "a message came in here" and "send this response back out there".

use uuid::Uuid;

/// Who sent an inbound message. `id` is the platform-stable user identifier
/// (Telegram user id, Discord user id, Slack `U…` id, WhatsApp phone, …) used
/// for allowlist checks; `display_name` is the human-friendly label woven into
/// the agent's context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderRef {
    pub id: String,
    pub display_name: String,
}

impl SenderRef {
    pub fn new(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
        }
    }
}

/// Context for a message that replies to a previous one, when the surface
/// exposes it (Telegram quote-reply, Discord reply, Slack thread parent).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReplyContext {
    /// Platform message id of the message being replied to.
    pub message_id: Option<String>,
    /// Text of the replied-to message, if the surface provided it.
    pub quoted_text: Option<String>,
}

/// A media attachment referenced by an inbound message. The gateway carries
/// the reference; surface-specific download/transcription has already produced
/// `text` (e.g. an STT transcript) when applicable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub kind: AttachmentKind,
    /// URL or local path the surface resolved for the attachment.
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    Image,
    Audio,
    Document,
    Other,
}

/// A message arriving from a surface, normalized for the gateway pipeline.
///
/// `conversation_key` is the platform-stable conversation identifier (chat id,
/// channel id, phone number, or — for the TUI — the session id rendered as a
/// string). It is what `gateway::services::session` keys session resolution on,
/// and what an [`Outbound`] carries back so the response lands in the same
/// conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inbound {
    pub surface_id: &'static str,
    pub conversation_key: String,
    pub sender: SenderRef,
    /// The agent-facing text. May be a wrapped form (sender metadata, reply
    /// context, group history) the surface assembled for the LLM.
    pub text: String,
    /// What the user literally typed, for DB / TUI display. `None` means use
    /// `text` for both.
    pub display_text: Option<String>,
    pub reply_ctx: Option<ReplyContext>,
    pub attachments: Vec<Attachment>,
    /// Surface-computed routing facts. The surface knows its own platform
    /// semantics (what a DM is, whether the bot was mentioned); the shared
    /// allowlist service applies the policy (`respond_to`, allowlists) on top.
    pub routing: Routing,
}

/// Platform-determined facts the shared allowlist policy needs. Each surface
/// fills these in because only it knows, e.g., what counts as a mention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Routing {
    /// True for a 1:1 / direct message. DMs always bypass `respond_to` and
    /// `allowed_channels` filtering, matching today's per-channel behavior.
    pub is_direct: bool,
    /// True when the bot was explicitly addressed (Discord/Slack @mention,
    /// Telegram reply-to-bot). Only consulted under `respond_to = mention`.
    pub is_mention: bool,
}

impl Default for Routing {
    fn default() -> Self {
        // Default models a direct message: always responded to. Group surfaces
        // override with the real facts.
        Self {
            is_direct: true,
            is_mention: false,
        }
    }
}

impl Inbound {
    /// Minimal constructor for the common "just text from a sender" case.
    /// Defaults [`Routing`] to a direct message (always responded to).
    pub fn new(
        surface_id: &'static str,
        conversation_key: impl Into<String>,
        sender: SenderRef,
        text: impl Into<String>,
    ) -> Self {
        Self {
            surface_id,
            conversation_key: conversation_key.into(),
            sender,
            text: text.into(),
            display_text: None,
            reply_ctx: None,
            attachments: Vec::new(),
            routing: Routing::default(),
        }
    }

    /// The text to persist / show, falling back to the agent text when no
    /// distinct display form was supplied.
    pub fn display(&self) -> &str {
        self.display_text.as_deref().unwrap_or(&self.text)
    }
}

/// Where an outbound response should be delivered. Mirrors the inbound
/// `conversation_key` plus any surface-specific routing hint (e.g. a Telegram
/// forum `thread_id`) the surface stashed on the way in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundTarget {
    pub conversation_key: String,
    /// Optional sub-routing within the conversation (thread/topic id).
    pub thread_key: Option<String>,
}

impl OutboundTarget {
    pub fn new(conversation_key: impl Into<String>) -> Self {
        Self {
            conversation_key: conversation_key.into(),
            thread_key: None,
        }
    }
}

/// The agent's response, ready for a surface to render. `voice` / `images` are
/// populated by the gateway's shared post-processing step (TTS synthesis, image
/// marker extraction) so individual surfaces don't each re-derive them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundMessage {
    pub text: String,
    /// Session the response belongs to (for surfaces that track session→chat).
    pub session_id: Uuid,
    /// Image URLs/paths extracted from the response text, if any.
    pub images: Vec<String>,
}

impl OutboundMessage {
    pub fn new(session_id: Uuid, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            session_id,
            images: Vec::new(),
        }
    }
}

/// A response routed back to the surface it originated from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outbound {
    pub surface_id: &'static str,
    pub target: OutboundTarget,
    pub message: OutboundMessage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbound_display_falls_back_to_text_when_no_display_text() {
        let inb = Inbound::new(
            "telegram",
            "12345",
            SenderRef::new("777", "Ada"),
            "wrapped: hello",
        );
        assert_eq!(inb.display(), "wrapped: hello");
    }

    #[test]
    fn inbound_display_prefers_display_text_when_set() {
        let mut inb = Inbound::new(
            "telegram",
            "12345",
            SenderRef::new("777", "Ada"),
            "wrapped: hello",
        );
        inb.display_text = Some("hello".to_string());
        assert_eq!(inb.display(), "hello");
        // The agent-facing text is unchanged.
        assert_eq!(inb.text, "wrapped: hello");
    }

    #[test]
    fn outbound_target_defaults_to_no_thread() {
        let t = OutboundTarget::new("chat-1");
        assert_eq!(t.conversation_key, "chat-1");
        assert_eq!(t.thread_key, None);
    }

    #[test]
    fn outbound_message_starts_with_no_images() {
        let m = OutboundMessage::new(Uuid::nil(), "hi");
        assert!(m.images.is_empty());
        assert_eq!(m.text, "hi");
    }
}
