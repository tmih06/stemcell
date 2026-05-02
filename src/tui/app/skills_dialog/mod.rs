//! Skills dialog — app-side state, input, actions.
//!
//! Pairs with `src/tui/render/skills_dialog/`. Single responsibility
//! per file: state lives in `state.rs`, key handling in `input.rs`,
//! side-effecting actions (open / execute) in `actions.rs`.

pub mod actions;
pub mod input;
pub mod state;

pub use state::{SkillsDialogState, matching};
