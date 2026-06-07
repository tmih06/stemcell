//! The cfg-gated surface registry — the **single** place where channel feature
//! flags appear.
//!
//! Every other part of the system (the gateway loop, the lifecycle manager,
//! `cli/ui.rs`) iterates the `Vec<Arc<dyn Surface>>` this returns and has zero
//! per-channel `#[cfg]` branches. A channel toggled off in `build_toggles.toml`
//! has its `pub mod` compiled out (in `channels/mod.rs`) and its client
//! dependency dropped (in `Cargo.toml`), so it contributes no source, no
//! symbols, and no registry entry — it is genuinely absent from the binary.
//!
//! Adding a channel = implement [`Surface`](super::surface::Surface), add one
//! `#[cfg(feature = "...")]` push here, and add the Cargo feature. No edits to
//! the manager, factory, or `ui.rs`.

use std::sync::Arc;

use super::surface::Surface;

/// Dependencies a surface needs to construct itself. Populated once at startup
/// in `cli/ui.rs` and passed to [`registered_surfaces`]. Fields are added as
/// surfaces are migrated onto the gateway; during early migration this carries
/// only what the already-migrated surfaces require.
#[derive(Clone)]
pub struct SurfaceDeps {
    pub agent: Arc<crate::brain::agent::AgentService>,
    pub service_context: crate::services::ServiceContext,
    pub config_rx: tokio::sync::watch::Receiver<crate::config::Config>,
    pub shared_session_id: Arc<tokio::sync::Mutex<Option<uuid::Uuid>>>,
    pub db_pool: deadpool_sqlite::Pool,
    /// The live TUI event sender, used by the TUI surface to route agent
    /// responses back to the terminal renderer.
    pub tui_event_tx: tokio::sync::mpsc::UnboundedSender<crate::tui::events::TuiEvent>,
    /// Shared Telegram state (connected Bot, owner chat, approval routing).
    /// Present only when the `telegram` feature is compiled in.
    #[cfg(feature = "telegram")]
    pub telegram_state: Arc<crate::channels::telegram::TelegramState>,
    /// Shared Discord state. Present only when `discord` is compiled in.
    #[cfg(feature = "discord")]
    pub discord_state: Arc<crate::channels::discord::DiscordState>,
    /// Shared Slack state. Present only when `slack` is compiled in.
    #[cfg(feature = "slack")]
    pub slack_state: Arc<crate::channels::slack::SlackState>,
    /// Shared WhatsApp state. Present only when `whatsapp` is compiled in.
    #[cfg(feature = "whatsapp")]
    pub whatsapp_state: Arc<crate::channels::whatsapp::WhatsAppState>,
    /// Shared Trello state. Present only when `trello` is compiled in.
    #[cfg(feature = "trello")]
    pub trello_state: Arc<crate::channels::trello::TrelloState>,
}

/// Build the list of surfaces present in this build. The TUI is always present;
/// each channel is gated on its feature.
///
/// During migration this starts empty and surfaces are added as they move onto
/// the [`Surface`] trait. The TUI is always present; each channel is gated on
/// its Cargo feature, so a channel toggled off in `build_toggles.toml`
/// contributes no source, no symbols, and no registry entry.
pub fn registered_surfaces(_deps: &SurfaceDeps) -> Vec<Arc<dyn Surface>> {
    #[allow(unused_mut)]
    let mut surfaces: Vec<Arc<dyn Surface>> = Vec::new();

    // The TUI is always compiled in — it is the local terminal frontend.
    surfaces.push(
        crate::channels::tui_surface::TuiSurface::new(_deps)
            .with_event_sender(_deps.tui_event_tx.clone())
            .into_arc(),
    );

    // Each channel surface, gated on its feature — the single source-exclusion
    // point for channels.
    #[cfg(feature = "telegram")]
    surfaces.push(
        crate::channels::telegram_surface::TelegramSurface::new(
            _deps,
            _deps.telegram_state.clone(),
        )
        .into_arc(),
    );

    #[cfg(feature = "discord")]
    surfaces.push(
        crate::channels::discord_surface::DiscordSurface::new(_deps, _deps.discord_state.clone())
            .into_arc(),
    );

    #[cfg(feature = "slack")]
    surfaces.push(
        crate::channels::slack_surface::SlackSurface::new(_deps, _deps.slack_state.clone())
            .into_arc(),
    );

    #[cfg(feature = "whatsapp")]
    surfaces.push(
        crate::channels::whatsapp_surface::WhatsAppSurface::new(
            _deps,
            _deps.whatsapp_state.clone(),
        )
        .into_arc(),
    );

    #[cfg(feature = "trello")]
    surfaces.push(
        crate::channels::trello_surface::TrelloSurface::new(_deps, _deps.trello_state.clone())
            .into_arc(),
    );

    surfaces
}
