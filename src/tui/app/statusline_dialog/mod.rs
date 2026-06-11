//! `/statusline` dialog — app-side state, input, actions.
//!
//! Pairs with `src/tui/render/statusline_dialog/`. Single responsibility
//! per file: state in `state.rs`, key handling in `input.rs`, side-effecting
//! actions (open) in `actions.rs`. Mirrors the `skills_dialog` layout.

pub mod actions;
pub mod input;
pub mod state;

pub use state::{FIELDS, StatusLineDialogState};
