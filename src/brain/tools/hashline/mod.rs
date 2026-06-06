//! Hashline editing module.
//!
//! Provides hash-anchored file editing where lines are referenced by 2-char
//! content hashes instead of reproduced text.

#[cfg(feature = "tool-hashline-edit")]
pub mod edit;
pub mod hash;
pub mod types;

#[cfg(feature = "tool-hashline-edit")]
pub use edit::HashlineEditTool;
