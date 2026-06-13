//! Built-in startup jobs.

pub mod brain_files;
pub mod check_config;
pub mod check_envs;
pub mod fetch_models;
pub mod rsi_digest;
pub mod rsi_proposals;
pub mod rsi_status;
pub mod tools_loaded;

pub use brain_files::BrainFilesJob;
pub use check_config::CheckConfigJob;
pub use check_envs::CheckEnvsJob;
pub use fetch_models::FetchModelsJob;
pub use rsi_digest::RsiDigestJob;
pub use rsi_proposals::RsiProposalsJob;
pub use rsi_status::RsiStatusJob;
pub use tools_loaded::ToolsLoadedJob;
