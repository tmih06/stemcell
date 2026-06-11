//! WhatsApp QR pairing helpers.
//!
//! Relocated out of the former `brain::tools::whatsapp_connect` agent tool when
//! channels stopped being agent tools (gateway refactor). These are used by the
//! TUI onboarding flow and dialogs — not the agent — to render the pairing QR
//! and subscribe to the running WhatsApp agent's QR / connected / error events.
//! No bot is ever created here; the gateway's `WhatsAppSurface` owns the single
//! bot instance and these helpers just observe its `WhatsAppState`.

use std::sync::Arc;

use qrcode::QrCode;

use crate::config::stemcell_home;

/// Render a QR code as pure Unicode block characters (no ANSI escapes).
/// Uses upper/lower half blocks to pack two rows per line. Includes a 4-module
/// quiet zone (white border) required for scanning.
pub fn render_qr_unicode(data: &str) -> Option<String> {
    let code = QrCode::new(data.as_bytes()).ok()?;
    let matrix = code.to_colors();
    let w = code.width();
    let quiet = 4;
    let total = w + quiet * 2;
    let mut out = String::new();

    let color_at = |x: usize, y: usize| -> qrcode::Color {
        if x < quiet || x >= quiet + w || y < quiet || y >= quiet + w {
            qrcode::Color::Light
        } else {
            matrix[(y - quiet) * w + (x - quiet)]
        }
    };

    let mut y = 0;
    while y < total {
        for x in 0..total {
            let top = color_at(x, y);
            let bot = if y + 1 < total {
                color_at(x, y + 1)
            } else {
                qrcode::Color::Light
            };
            // Inverted mapping: light modules = bright block, dark modules =
            // space. This is the qrencode -t UTF8 convention — white blocks on a
            // dark terminal background — which phone cameras read reliably.
            let ch = match (top, bot) {
                (qrcode::Color::Light, qrcode::Color::Light) => '\u{2588}', // full bright
                (qrcode::Color::Dark, qrcode::Color::Dark) => ' ',          // transparent dark
                (qrcode::Color::Light, qrcode::Color::Dark) => '\u{2580}',  // upper bright
                (qrcode::Color::Dark, qrcode::Color::Light) => '\u{2584}',  // lower bright
            };
            out.push(ch);
        }
        out.push('\n');
        y += 2;
    }
    Some(out)
}

/// Handle returned by [`subscribe_whatsapp_pairing`] for QR / connection events.
/// No bot is created — subscribes to the single agent bot via `WhatsAppState`.
pub struct WhatsAppConnectHandle {
    /// Receives QR code data strings from the agent bot.
    pub qr_rx: tokio::sync::broadcast::Receiver<String>,
    /// Fires once when WhatsApp connects.
    pub connected_rx: tokio::sync::broadcast::Receiver<()>,
    /// Receives error messages from the agent bot.
    pub error_rx: tokio::sync::broadcast::Receiver<String>,
    /// Shared WhatsApp state — use `client()` after connected for test messages.
    pub wa_state: Arc<crate::channels::whatsapp::WhatsAppState>,
}

/// Subscribe to QR / connected events from the running WhatsApp agent bot.
/// Does NOT create a new bot — the gateway's `WhatsAppSurface` is the only
/// instance. If `wipe_session` is true, deletes session.db first so the agent
/// shows a fresh QR.
pub fn subscribe_whatsapp_pairing(
    wa_state: &Arc<crate::channels::whatsapp::WhatsAppState>,
    wipe_session: bool,
) -> WhatsAppConnectHandle {
    if wipe_session {
        let wa_dir = stemcell_home().join("whatsapp");
        let _ = std::fs::remove_file(wa_dir.join("session.db"));
        let _ = std::fs::remove_file(wa_dir.join("session.db-wal"));
        let _ = std::fs::remove_file(wa_dir.join("session.db-shm"));
    }

    WhatsAppConnectHandle {
        qr_rx: wa_state.subscribe_qr(),
        connected_rx: wa_state.subscribe_connected(),
        error_rx: wa_state.subscribe_error(),
        wa_state: wa_state.clone(),
    }
}
