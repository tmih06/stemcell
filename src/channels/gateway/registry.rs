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
}

/// Build the list of surfaces present in this build. The TUI is always present;
/// each channel is gated on its feature.
///
/// During migration this starts empty and surfaces are added as they move onto
/// the [`Surface`] trait:
/// - Phase 2: the TUI surface (always present).
/// - Phase 3: Telegram (gated on `feature = "telegram"`).
/// - Phase 4: Discord / Slack / WhatsApp / Trello.
///
/// Un-migrated channels continue to run via the legacy `ChannelManager` path,
/// untouched, so the build stays green at every step.
pub fn registered_surfaces(_deps: &SurfaceDeps) -> Vec<Arc<dyn Surface>> {
    #[allow(unused_mut)]
    let mut surfaces: Vec<Arc<dyn Surface>> = Vec::new();

    // Surfaces are pushed here as they are migrated. See the doc comment above.

    surfaces
}
