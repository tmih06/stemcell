//! Shared, surface-agnostic services for the gateway pipeline.
//!
//! These hold the cross-cutting logic that every channel used to re-implement:
//! allowlist / respond-to policy and session resolution. Surfaces supply
//! platform facts; these services apply the shared rules.

pub mod allowlist;
pub mod session;
