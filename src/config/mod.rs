//! Configuration Module
//!
//! Handles application configuration loading, validation, and management.

pub mod crabrace;
pub mod health;
pub mod profile;
pub mod secrets;
mod types;
pub mod update;

pub use crabrace::{CrabraceConfig, CrabraceIntegration};
pub use secrets::SecretString;
pub use types::*;
pub use update::{ProviderUpdater, UpdateResult};

// `merge_provider_keys` is internal to the crate but must be reachable
// from regression tests and the startup model-warming job.
pub(crate) use types::merge_provider_keys;
