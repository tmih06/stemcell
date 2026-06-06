//! Tool Execution Framework
//!
//! Provides an abstraction for tools that can be called by LLM agents,
//! including file operations, shell commands, and more.
//!
//! # Compile-time feature flags
//!
//! Tool modules can be excluded at compile time via Cargo features to reduce
//! binary size. All tool features are enabled by default. To build a minimal
//! binary, use `--no-default-features` and enable only what you need:
//!
//! ```sh
//! cargo build --no-default-features --features "telegram,tools-file-ops"
//! ```

pub mod brain_file_safety;
pub mod error;
pub mod registry;
mod r#trait;

pub mod fuzzy;

// Tool implementations - Phase 1: Essential File Operations (tools-file-ops)
#[cfg(feature = "tools-file-ops")]
pub mod bash;
#[cfg(feature = "tools-file-ops")]
pub mod edit;
#[cfg(feature = "tools-file-ops")]
pub mod glob;
#[cfg(feature = "tools-file-ops")]
pub mod grep;
#[cfg(feature = "tools-file-ops")]
pub mod hashline;
#[cfg(feature = "tools-file-ops")]
pub mod ls;
#[cfg(feature = "tools-file-ops")]
pub mod read;
#[cfg(feature = "tools-file-ops")]
pub mod write;

// Tool implementations - Phase 2: Advanced Features (tools-search / tools-workflow)
#[cfg(feature = "tools-search")]
pub mod brave_search;
#[cfg(feature = "tools-workflow")]
pub mod code_exec;
#[cfg(feature = "tools-workflow")]
pub mod doc_parser;
#[cfg(feature = "tools-search")]
pub mod exa_search;
#[cfg(feature = "tools-workflow")]
pub mod notebook;
#[cfg(feature = "tools-search")]
pub mod web_search;

// Tool implementations - Phase 3: Workflow & Integration (tools-workflow / tools-image / tools-brain)
#[cfg(feature = "tools-brain")]
pub mod a2a_send;
#[cfg(feature = "tools-image")]
pub mod analyze_image;
#[cfg(feature = "tools-image")]
pub mod analyze_video;

// Tool implementations - Recursive Self-Improvement (tools-rsi / tools-search / tools-workflow / tools-brain / tools-image / tools-meta)
#[cfg(feature = "tools-search")]
pub mod channel_search;
#[cfg(feature = "tools-workflow")]
pub mod config_tool;
#[cfg(feature = "tools-workflow")]
pub mod context;
#[cfg(feature = "tools-workflow")]
pub mod cron_manage;
pub mod evolve;
#[cfg(feature = "tools-rsi")]
pub mod feedback_analyze;
#[cfg(feature = "tools-rsi")]
pub mod feedback_record;
#[cfg(feature = "tools-workflow")]
pub mod follow_up_question;
#[cfg(feature = "tools-image")]
pub mod generate_image;
#[cfg(feature = "tools-workflow")]
pub mod http;
#[cfg(feature = "tools-brain")]
pub mod load_brain_file;
#[cfg(feature = "tools-search")]
pub mod memory_search;
#[cfg(feature = "tools-workflow")]
pub mod plan_tool;
#[cfg(feature = "tools-image")]
pub mod provider_vision;
pub mod rebuild;
#[cfg(feature = "tools-brain")]
pub mod rename_session;
#[cfg(feature = "tools-meta")]
pub mod rsi_proposals;
#[cfg(feature = "tools-rsi")]
pub mod rsi_propose;
#[cfg(feature = "tools-rsi")]
pub mod self_improve;
#[cfg(feature = "tools-search")]
pub mod session_search;
#[cfg(feature = "tools-brain")]
pub mod slash_command;
#[cfg(feature = "tools-workflow")]
pub mod task;
#[cfg(feature = "tools-brain")]
pub mod write_opencrabs_file;

// Tool implementations - Phase 5: Multi-Agent Orchestration (tools-multi-agent)
#[cfg(feature = "tools-multi-agent")]
pub mod subagent;

// Dynamic tools — runtime-defined via tools.toml (tools-dynamic)
#[cfg(feature = "tools-dynamic")]
pub mod dynamic;
#[cfg(feature = "tools-meta")]
pub mod tool_manage;

// Modular tool architecture — groups tools into disableable modules
pub mod modules;

// Browser automation — headless Chrome via CDP (tools-browser)
#[cfg(feature = "tools-browser")]
pub mod browser;

// Tool implementations - Phase 4: Channel Integrations
// Channel tools are gated on their respective channel features.
// The tools-channel-integrations feature controls the ChannelIntegrationsModule
// registration, but the tool code itself is always available when the channel is enabled.
#[cfg(feature = "discord")]
pub mod discord_connect;
#[cfg(feature = "discord")]
pub mod discord_send;
#[cfg(feature = "slack")]
pub mod slack_connect;
#[cfg(feature = "slack")]
pub mod slack_send;
#[cfg(feature = "telegram")]
pub mod telegram_connect;
#[cfg(feature = "telegram")]
pub mod telegram_send;
#[cfg(feature = "trello")]
pub mod trello_connect;
#[cfg(feature = "trello")]
pub mod trello_send;
#[cfg(feature = "whatsapp")]
pub mod whatsapp_connect;
#[cfg(feature = "whatsapp")]
pub mod whatsapp_send;

// Re-exports
pub use error::{Result, ToolError};
pub use registry::ToolRegistry;
pub use r#trait::{Tool, ToolCapability, ToolExecutionContext, ToolResult};
