//! `/export` dialog — app-side state, input, actions.
//!
//! Pairs with `src/tui/render/export_dialog/`. Single responsibility per
//! file: state in `state.rs`, key handling in `input.rs`, side-effecting
//! actions (open) in `actions.rs`. Mirrors the `statusline_dialog` layout.

pub mod actions;
pub mod input;
pub mod state;

pub use state::{EXPORT_OPTIONS, ExportDialogState, ExportTarget};
