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
#[cfg(feature = "tool-bash")]
pub mod bash;
#[cfg(feature = "tool-edit")]
pub mod edit;
#[cfg(feature = "tool-glob")]
pub mod glob;
#[cfg(feature = "tool-grep")]
pub mod grep;
pub mod hashline;
#[cfg(feature = "tool-ls")]
pub mod ls;
#[cfg(feature = "tool-read")]
pub mod read;
#[cfg(feature = "tool-write")]
pub mod write;

// Tool implementations - Phase 2: Advanced Features (tools-search / tools-workflow)
#[cfg(feature = "tool-brave-search")]
pub mod brave_search;
#[cfg(feature = "tool-execute-code")]
pub mod code_exec;
#[cfg(feature = "tool-parse-document")]
pub mod doc_parser;
#[cfg(feature = "tool-exa-search")]
pub mod exa_search;
#[cfg(feature = "tool-notebook-edit")]
pub mod notebook;
#[cfg(feature = "tool-web-search")]
pub mod web_search;

// Tool implementations - Phase 3: Workflow & Integration (tools-workflow / tools-image / tools-brain)
#[cfg(feature = "tool-a2a-send")]
pub mod a2a_send;
#[cfg(feature = "tool-analyze-image")]
pub mod analyze_image;
#[cfg(feature = "tool-analyze-video")]
pub mod analyze_video;

// Tool implementations - Recursive Self-Improvement (tools-rsi / tools-search / tools-workflow / tools-brain / tools-image / tools-meta)
#[cfg(feature = "tool-channel-search")]
pub mod channel_search;
#[cfg(feature = "tool-config-manager")]
pub mod config_tool;
#[cfg(feature = "tool-session-context")]
pub mod context;
#[cfg(feature = "tool-cron-manage")]
pub mod cron_manage;
#[cfg(feature = "tool-evolve")]
pub mod evolve;
#[cfg(feature = "tool-feedback-analyze")]
pub mod feedback_analyze;
#[cfg(feature = "tool-feedback-record")]
pub mod feedback_record;
#[cfg(feature = "tool-follow-up-question")]
pub mod follow_up_question;
#[cfg(feature = "tool-generate-image")]
pub mod generate_image;
#[cfg(feature = "tool-http-request")]
pub mod http;
#[cfg(feature = "tool-load-brain-file")]
pub mod load_brain_file;
#[cfg(feature = "tool-memory-search")]
pub mod memory_search;
#[cfg(feature = "tool-plan")]
pub mod plan_tool;
#[cfg(feature = "tool-analyze-image")]
pub mod provider_vision;
#[cfg(feature = "tool-rebuild")]
pub mod rebuild;
#[cfg(feature = "tool-rename-session")]
pub mod rename_session;
#[cfg(feature = "tool-rsi-proposals")]
pub mod rsi_proposals;
#[cfg(feature = "tool-rsi-propose")]
pub mod rsi_propose;
#[cfg(feature = "tool-self-improve")]
pub mod self_improve;
#[cfg(feature = "tool-session-search")]
pub mod session_search;
#[cfg(feature = "tool-slash-command")]
pub mod slash_command;
#[cfg(feature = "tool-task-manager")]
pub mod task;
#[cfg(feature = "tool-write-stemcell-file")]
pub mod write_stemcell_file;

// Tool implementations - Phase 5: Multi-Agent Orchestration (tools-multi-agent)
#[cfg(any(
    feature = "tool-spawn-agent",
    feature = "tool-wait-agent",
    feature = "tool-send-input",
    feature = "tool-close-agent",
    feature = "tool-resume-agent",
    feature = "tool-team-create",
    feature = "tool-team-delete",
    feature = "tool-team-broadcast"
))]
pub mod subagent;

// Dynamic tools — runtime-defined via tools.toml (tools-dynamic)
#[cfg(feature = "tools-dynamic")]
pub mod dynamic;
#[cfg(feature = "tool-tool-manage")]
pub mod tool_manage;

// Modular tool architecture — groups tools into disableable modules
pub mod modules;

// Browser automation — headless Chrome via CDP (tools-browser)
#[cfg(any(
    feature = "tool-browser-navigate",
    feature = "tool-browser-screenshot",
    feature = "tool-browser-click",
    feature = "tool-browser-type",
    feature = "tool-browser-eval",
    feature = "tool-browser-content",
    feature = "tool-browser-wait",
    feature = "tool-browser-find",
    feature = "tool-browser-close"
))]
pub mod browser;

// Channels are no longer agent tools. Inbound messages from a channel enter the
// agent like a TUI prompt and the gateway routes the response back out the
// originating surface — the agent has no telegram_send / *_connect tools and
// nothing channel-related in its context. See `crate::channels::gateway`.

// Re-exports
pub use error::{Result, ToolError};
pub use registry::ToolRegistry;
pub use r#trait::{Tool, ToolCapability, ToolExecutionContext, ToolResult, parse_input};
