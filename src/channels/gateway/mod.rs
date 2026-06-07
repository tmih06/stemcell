//! Unified channel gateway.
//!
//! Channels are not agent tools — they are **remote surfaces**. A message
//! arriving from Telegram (or Discord, or the TUI) enters the agent exactly
//! like a TUI prompt; the agent replies with its normal loop, knowing nothing
//! about channels; and the gateway routes the response back out the surface it
//! came from. TUI and every channel are peer surfaces on one async bus.
//!
//! ## Modules
//! - [`envelope`] — normalized [`Inbound`]/[`Outbound`] message types.
//! - [`surface`] — the [`Surface`] trait every frontend implements.
//! - [`bus`] — the [`Gateway`] run loop + [`GatewayHandle`] producer.
//! - [`registry`] — the single cfg-gated list of compiled-in surfaces.
//! - [`services`] — shared allowlist + session logic.

pub mod bus;
pub mod envelope;
pub mod registry;
pub mod services;
pub mod surface;

pub use bus::{Gateway, GatewayContext, GatewayHandle};
pub use envelope::{
    Attachment, AttachmentKind, Inbound, Outbound, OutboundMessage, OutboundTarget, ReplyContext,
    Routing, SenderRef,
};
pub use registry::{registered_surfaces, SurfaceDeps};
pub use surface::{Surface, SurfaceStatus};
