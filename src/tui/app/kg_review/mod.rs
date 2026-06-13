//! `/kg` review screen — app-side state, input, actions.
//!
//! Pairs with `src/brain/kg/review.rs` (the git/queue service) and
//! `src/tui/render/kg_review/` (renderer). State in `state.rs`, key handling in
//! `input.rs`, side-effecting service calls in `actions.rs`. Mirrors the
//! Mission Control layering.

pub mod actions;
pub mod input;
pub mod state;

pub use state::{KgReviewState, KgView};
