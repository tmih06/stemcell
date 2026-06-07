//! Shared channel-session resolution for the gateway.
//!
//! Re-exports the existing suffix-stable resolver (formerly
//! `channels::session_resolve` / `channels::session_init`) and adds
//! [`resolve_for_inbound`], the single entry point the bus uses to map an
//! [`Inbound`] to a session id.
//!
//! The resolver keys on `surface_id + conversation_key`, building the same
//! `[chat:<id>]` suffix every channel already used, so sessions survive the
//! agent's auto-rename and don't duplicate per turn (issue #121 / PR #123).

use anyhow::Result;
use uuid::Uuid;

use crate::channels::gateway::envelope::Inbound;
use crate::config::Config;
use crate::services::SessionService;

// The lower-level helpers still live in their original modules during the
// migration; the gateway exposes them here so callers depend on
// `gateway::services::session` rather than the old paths.
pub use crate::channels::session_init::create_channel_session;
pub use crate::channels::session_resolve::{
    chat_id_suffix, resolve_or_create_channel_session, session_idle_expired,
};

/// Resolve (or create) the session for an inbound message.
///
/// - The **TUI** surface carries the session id directly in
///   `conversation_key`, so it parses straight through with no DB lookup.
/// - **Channels** resolve via the suffix-stable lookup keyed on
///   `surface_id:conversation_key`, creating a new session when none exists and
///   honoring the per-channel idle timeout.
pub async fn resolve_for_inbound(
    session_svc: &SessionService,
    inbound: &Inbound,
    cfg: &Config,
) -> Result<Uuid> {
    // The TUI addresses a session directly — its conversation_key IS the
    // session id. No lookup, no creation here.
    if inbound.surface_id == "tui"
        && let Ok(id) = Uuid::parse_str(&inbound.conversation_key)
    {
        return Ok(id);
    }

    let suffix = chat_id_suffix(&inbound.conversation_key);
    let title = format!(
        "{} chat with {} {}",
        inbound.surface_id, inbound.sender.display_name, suffix
    );
    // Legacy pre-suffix title shape used by the old per-channel resolvers.
    let legacy_title = format!(
        "{} chat with {}",
        inbound.surface_id, inbound.sender.display_name
    );
    let idle_hours = idle_hours_for(inbound.surface_id, cfg);

    resolve_or_create_channel_session(
        session_svc,
        &suffix,
        &legacy_title,
        &title,
        idle_hours,
        inbound.surface_id,
    )
    .await
}

/// Read the per-surface idle timeout from config (channels that expose one).
fn idle_hours_for(surface_id: &str, cfg: &Config) -> Option<f64> {
    let c = &cfg.channels;
    match surface_id {
        "telegram" => c.telegram.session_idle_hours,
        "discord" => c.discord.session_idle_hours,
        "slack" => c.slack.session_idle_hours,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::gateway::envelope::SenderRef;

    #[test]
    fn tui_conversation_key_parses_as_session_id() {
        // The TUI fast-path is pure parsing; we can assert it without a DB by
        // checking that a valid uuid conversation_key round-trips.
        let id = Uuid::new_v4();
        let inb = Inbound::new("tui", id.to_string(), SenderRef::new("u", "U"), "hi");
        // Mirror the fast-path branch condition.
        assert_eq!(inb.surface_id, "tui");
        assert_eq!(Uuid::parse_str(&inb.conversation_key).unwrap(), id);
    }

    #[test]
    fn idle_hours_reads_per_surface_config() {
        let mut cfg = Config::default();
        cfg.channels.telegram.session_idle_hours = Some(4.0);
        assert_eq!(idle_hours_for("telegram", &cfg), Some(4.0));
        assert_eq!(idle_hours_for("whatsapp", &cfg), None);
        assert_eq!(idle_hours_for("tui", &cfg), None);
    }
}
