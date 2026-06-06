//! Modular Tool Architecture
//!
//! Each tool module groups related tools and can be individually disabled via
//! `config.toml` to reduce token bloat. Modules register their tools into a
//! shared `ToolRegistry` during startup.
//!
//! # Compile-time vs Runtime disabling
//!
//! **Compile-time** (Cargo features): Exclude module code from the binary entirely.
//! Reduces binary size and compile time. Use `--no-default-features` and enable
//! only the features you need:
//! ```sh
//! cargo build --no-default-features --features "telegram,tools-file-ops,tools-search"
//! ```
//!
//! **Runtime** (config.toml): Skip module registration at startup. The code is
//! still in the binary but tools don't appear in the LLM tool list:
//! ```toml
//! [tools]
//! disabled = ["browser", "rsi", "channel_integrations"]
//! ```
//!
//! # Adding a new tool module
//!
//! 1. Implement the [`ToolModule`] trait for a unit struct
//! 2. Add it to [`all_modules()`]
//! 3. That's it — the module is now available for enable/disable in config

use crate::config::Config;
use crate::db::Pool;
use std::collections::HashSet;
use std::sync::Arc;

use super::registry::ToolRegistry;
#[cfg(feature = "tools-multi-agent")]
use super::subagent::{SubAgentManager, TeamManager};

/// Registration mode controls which tools are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistrationMode {
    /// Full mode: all tools including TUI-only tools (brain file loader,
    /// channel search, cron manage, A2A, image/vision, etc.)
    Full,
    /// Minimal mode: core tools only, for non-interactive CLI usage.
    Minimal,
}

/// Dependencies and context passed to each module during registration.
pub struct ModuleContext {
    pub registry: Arc<ToolRegistry>,
    pub config: Config,
    pub pool: Pool,
    pub mode: RegistrationMode,
    #[cfg(feature = "tools-multi-agent")]
    pub subagent_manager: Arc<SubAgentManager>,
    #[cfg(feature = "tools-multi-agent")]
    pub team_manager: Arc<TeamManager>,
}

/// A tool module is a logical grouping of related tools that can be
/// enabled or disabled as a unit.
pub trait ToolModule: Send + Sync {
    /// Unique module identifier used in config (snake_case).
    fn id(&self) -> &str;

    /// Human-readable name for log messages.
    fn name(&self) -> &str;

    /// Short description of what this module provides.
    fn description(&self) -> &str;

    /// Register this module's tools into the registry.
    fn register(&self, ctx: &ModuleContext);

    /// Whether this module is enabled by default. Modules that require
    /// external services or feature flags may default to false.
    fn enabled_by_default(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Module definitions
// ---------------------------------------------------------------------------

/// File operations: read, write, edit, hashline_edit, bash, ls, glob, grep
#[cfg(feature = "tools-file-ops")]
struct FileOpsModule;

#[cfg(feature = "tools-file-ops")]
impl ToolModule for FileOpsModule {
    fn id(&self) -> &str {
        "file_ops"
    }
    fn name(&self) -> &str {
        "File Operations"
    }
    fn description(&self) -> &str {
        "Core file I/O, shell execution, and code navigation tools"
    }
    fn register(&self, ctx: &ModuleContext) {
        use super::{
            bash::BashTool, edit::EditTool, glob::GlobTool, grep::GrepTool,
            hashline::HashlineEditTool, ls::LsTool, read::ReadTool, write::WriteTool,
        };
        ctx.registry.register(Arc::new(ReadTool));
        ctx.registry.register(Arc::new(WriteTool));
        ctx.registry.register(Arc::new(EditTool));
        ctx.registry.register(Arc::new(HashlineEditTool));
        ctx.registry.register(Arc::new(BashTool));
        ctx.registry.register(Arc::new(LsTool));
        ctx.registry.register(Arc::new(GlobTool));
        ctx.registry.register(Arc::new(GrepTool));
    }
}

/// Search & memory: web_search, exa_search, brave_search, memory_search,
/// session_search, channel_search
#[cfg(feature = "tools-search")]
struct SearchModule;

#[cfg(feature = "tools-search")]
impl ToolModule for SearchModule {
    fn id(&self) -> &str {
        "search"
    }
    fn name(&self) -> &str {
        "Search & Memory"
    }
    fn description(&self) -> &str {
        "Web search, semantic memory search, and session history search"
    }
    fn register(&self, ctx: &ModuleContext) {
        use super::{
            brave_search::BraveSearchTool, exa_search::ExaSearchTool,
            memory_search::MemorySearchTool, session_search::SessionSearchTool,
            web_search::WebSearchTool,
        };

        ctx.registry.register(Arc::new(WebSearchTool));
        ctx.registry.register(Arc::new(MemorySearchTool));
        ctx.registry
            .register(Arc::new(SessionSearchTool::new(ctx.pool.clone())));

        // EXA search: always available (free via MCP), uses direct API if key is set
        let exa_key = ctx
            .config
            .providers
            .web_search
            .as_ref()
            .and_then(|ws| ws.exa.as_ref())
            .and_then(|p| p.api_key.clone())
            .filter(|k| !k.is_empty());
        let exa_mode = if exa_key.is_some() {
            "direct API"
        } else {
            "MCP (free)"
        };
        ctx.registry.register(Arc::new(ExaSearchTool::new(exa_key)));
        tracing::info!("Registered EXA search tool (mode: {})", exa_mode);

        // Brave search: requires enabled = true in config.toml AND API key
        if let Some(brave_cfg) = ctx
            .config
            .providers
            .web_search
            .as_ref()
            .and_then(|ws| ws.brave.as_ref())
            && brave_cfg.enabled
            && let Some(brave_key) = brave_cfg.api_key.clone()
        {
            ctx.registry
                .register(Arc::new(BraveSearchTool::new(brave_key)));
            tracing::info!("Registered Brave search tool");
        }

        // Channel search — only in Full mode
        if ctx.mode == RegistrationMode::Full {
            use super::channel_search::ChannelSearchTool;
            ctx.registry.register(Arc::new(ChannelSearchTool::new(
                crate::db::ChannelMessageRepository::new(ctx.pool.clone()),
            )));
        }
    }
}

/// Workflow & integration: task, plan, context, config, cron, notebook,
/// doc_parser, http, code_exec, follow_up_question
#[cfg(feature = "tools-workflow")]
struct WorkflowModule;

#[cfg(feature = "tools-workflow")]
impl ToolModule for WorkflowModule {
    fn id(&self) -> &str {
        "workflow"
    }
    fn name(&self) -> &str {
        "Workflow & Integration"
    }
    fn description(&self) -> &str {
        "Task management, planning, HTTP client, code execution, and document parsing"
    }
    fn register(&self, ctx: &ModuleContext) {
        use super::{
            code_exec::CodeExecTool, config_tool::ConfigTool, context::ContextTool,
            doc_parser::DocParserTool, follow_up_question::FollowUpQuestionTool,
            http::HttpClientTool, notebook::NotebookEditTool, plan_tool::PlanTool, task::TaskTool,
        };
        ctx.registry.register(Arc::new(TaskTool));
        ctx.registry.register(Arc::new(ContextTool));
        ctx.registry.register(Arc::new(HttpClientTool));
        ctx.registry.register(Arc::new(PlanTool));
        ctx.registry.register(Arc::new(CodeExecTool));
        ctx.registry.register(Arc::new(NotebookEditTool));
        ctx.registry.register(Arc::new(DocParserTool));
        ctx.registry.register(Arc::new(ConfigTool));
        ctx.registry.register(Arc::new(FollowUpQuestionTool));

        // Cron job management — Full mode only
        if ctx.mode == RegistrationMode::Full {
            use super::cron_manage::CronManageTool;
            ctx.registry.register(Arc::new(CronManageTool::new(
                crate::db::CronJobRepository::new(ctx.pool.clone()),
            )));
        }
    }
}

/// Multi-agent orchestration: spawn, wait, send_input, close, resume + team tools
#[cfg(feature = "tools-multi-agent")]
struct MultiAgentModule;

#[cfg(feature = "tools-multi-agent")]
impl ToolModule for MultiAgentModule {
    fn id(&self) -> &str {
        "multi_agent"
    }
    fn name(&self) -> &str {
        "Multi-Agent Orchestration"
    }
    fn description(&self) -> &str {
        "Sub-agent spawning, team management, and inter-agent communication"
    }
    fn register(&self, ctx: &ModuleContext) {
        use super::subagent::{
            CloseAgentTool, ResumeAgentTool, SendInputTool, SpawnAgentTool, TeamBroadcastTool,
            TeamCreateTool, TeamDeleteTool, WaitAgentTool,
        };

        ctx.registry.register(Arc::new(SpawnAgentTool::new(
            ctx.subagent_manager.clone(),
            ctx.registry.clone(),
        )));
        ctx.registry
            .register(Arc::new(WaitAgentTool::new(ctx.subagent_manager.clone())));
        ctx.registry
            .register(Arc::new(SendInputTool::new(ctx.subagent_manager.clone())));
        ctx.registry
            .register(Arc::new(CloseAgentTool::new(ctx.subagent_manager.clone())));
        ctx.registry.register(Arc::new(ResumeAgentTool::new(
            ctx.subagent_manager.clone(),
            ctx.registry.clone(),
        )));

        ctx.registry.register(Arc::new(TeamCreateTool::new(
            ctx.subagent_manager.clone(),
            ctx.team_manager.clone(),
            ctx.registry.clone(),
        )));
        ctx.registry.register(Arc::new(TeamDeleteTool::new(
            ctx.subagent_manager.clone(),
            ctx.team_manager.clone(),
        )));
        ctx.registry.register(Arc::new(TeamBroadcastTool::new(
            ctx.subagent_manager.clone(),
            ctx.team_manager.clone(),
        )));
    }
}

/// RSI (Recursive Self-Improvement): feedback_record, feedback_analyze, self_improve
#[cfg(feature = "tools-rsi")]
struct RsiModule;

#[cfg(feature = "tools-rsi")]
impl ToolModule for RsiModule {
    fn id(&self) -> &str {
        "rsi"
    }
    fn name(&self) -> &str {
        "Recursive Self-Improvement"
    }
    fn description(&self) -> &str {
        "Feedback recording, analysis, and autonomous self-improvement"
    }
    fn register(&self, ctx: &ModuleContext) {
        use super::{
            feedback_analyze::FeedbackAnalyzeTool, feedback_record::FeedbackRecordTool,
            self_improve::SelfImproveTool,
        };
        ctx.registry.register(Arc::new(FeedbackRecordTool));
        ctx.registry.register(Arc::new(FeedbackAnalyzeTool));
        ctx.registry.register(Arc::new(SelfImproveTool));
    }
}

/// Image & vision: generate_image, analyze_image, provider_vision, analyze_video
#[cfg(feature = "tools-image")]
struct ImageModule;

#[cfg(feature = "tools-image")]
impl ToolModule for ImageModule {
    fn id(&self) -> &str {
        "image"
    }
    fn name(&self) -> &str {
        "Image & Vision"
    }
    fn description(&self) -> &str {
        "Image generation, vision analysis, and video understanding"
    }
    fn register(&self, ctx: &ModuleContext) {
        use super::{
            analyze_image::AnalyzeImageTool, analyze_video::AnalyzeVideoTool,
            generate_image::GenerateImageTool, provider_vision::ProviderVisionTool,
        };

        // Image generation
        if let Some(tool) = GenerateImageTool::from_config(&ctx.config) {
            ctx.registry.register(Arc::new(tool));
            tracing::info!("Registered generate_image tool");
        }

        // Vision: provider.vision_model takes priority over image.vision (Gemini)
        if let Some((api_key, base_url, vision_model)) =
            crate::brain::provider::factory::active_provider_vision(&ctx.config)
        {
            ctx.registry.register(Arc::new(ProviderVisionTool::new(
                api_key,
                base_url,
                vision_model,
            )));
            tracing::info!("Registered analyze_image tool (provider vision model)");
        } else if ctx.config.image.vision.enabled
            && let Some(ref key) = ctx.config.image.vision.api_key
        {
            ctx.registry.register(Arc::new(AnalyzeImageTool::new(
                key.clone(),
                ctx.config.image.vision.model.clone(),
            )));
            tracing::info!("Registered analyze_image tool (Gemini)");
        }

        // Video vision — Gemini-native multimodal video understanding
        if ctx.config.image.vision.enabled
            && let Some(ref key) = ctx.config.image.vision.api_key
            && !key.is_empty()
        {
            ctx.registry.register(Arc::new(AnalyzeVideoTool::new(
                key.clone(),
                ctx.config.image.vision.model.clone(),
            )));
            tracing::info!("Registered analyze_video tool (Gemini)");
        }
    }
}

/// Brain & session management: load_brain_file, write_opencrabs_file,
/// rename_session, slash_command, a2a_send
#[cfg(feature = "tools-brain")]
struct BrainModule;

#[cfg(feature = "tools-brain")]
impl ToolModule for BrainModule {
    fn id(&self) -> &str {
        "brain"
    }
    fn name(&self) -> &str {
        "Brain & Session Management"
    }
    fn description(&self) -> &str {
        "Brain file I/O, session management, slash commands, and A2A communication"
    }
    fn register(&self, ctx: &ModuleContext) {
        use super::{
            a2a_send::A2aSendTool, load_brain_file::LoadBrainFileTool,
            rename_session::RenameSessionTool, slash_command::SlashCommandTool,
            write_opencrabs_file::WriteOpenCrabsFileTool,
        };

        ctx.registry.register(Arc::new(SlashCommandTool));
        ctx.registry.register(Arc::new(RenameSessionTool));

        // Full mode only: brain file loader, opencrabs file writer, A2A
        if ctx.mode == RegistrationMode::Full {
            ctx.registry.register(Arc::new(LoadBrainFileTool));
            ctx.registry.register(Arc::new(WriteOpenCrabsFileTool));
            ctx.registry.register(Arc::new(A2aSendTool::new()));
        }
    }
}

/// Channel integrations: discord, slack, telegram, trello, whatsapp
/// (feature-gated)
///
/// Note: Channel connect/send tools are registered by the channel subsystem
/// when channels connect, not during startup tool registration. This module
/// exists as a documentation entry and potential future toggle point.
#[cfg(feature = "tools-channel-integrations")]
struct ChannelIntegrationsModule;

#[cfg(feature = "tools-channel-integrations")]
impl ToolModule for ChannelIntegrationsModule {
    fn id(&self) -> &str {
        "channel_integrations"
    }
    fn name(&self) -> &str {
        "Channel Integrations"
    }
    fn description(&self) -> &str {
        "Messaging platform connectors (Telegram, Discord, Slack, WhatsApp, Trello)"
    }
    fn register(&self, _ctx: &ModuleContext) {
        // Channel tools are registered by the channel subsystem itself
        // when channels connect (e.g., telegram handler, discord handler).
        // This module serves as a config toggle point — when the
        // "channel_integrations" module is disabled, the channel subsystem
        // can check this and skip tool registration.
        tracing::debug!(
            "Channel integrations module loaded (tools registered by channel subsystem)"
        );
    }
}

/// Browser automation tools (feature-gated)
#[cfg(feature = "tools-browser")]
struct BrowserModule;

#[cfg(feature = "tools-browser")]
impl ToolModule for BrowserModule {
    fn id(&self) -> &str {
        "browser"
    }
    fn name(&self) -> &str {
        "Browser Automation"
    }
    fn description(&self) -> &str {
        "Headless Chrome automation via CDP (navigate, click, type, screenshot, etc.)"
    }
    #[cfg(feature = "browser")]
    fn register(&self, ctx: &ModuleContext) {
        use super::browser::{
            BrowserClickTool, BrowserCloseTool, BrowserContentTool, BrowserEvalTool,
            BrowserFindTool, BrowserManager, BrowserNavigateTool, BrowserScreenshotTool,
            BrowserTypeTool, BrowserWaitTool,
        };

        let browser_manager = Arc::new(BrowserManager::new());
        ctx.registry
            .register(Arc::new(BrowserNavigateTool::new(browser_manager.clone())));
        ctx.registry.register(Arc::new(BrowserScreenshotTool::new(
            browser_manager.clone(),
        )));
        ctx.registry
            .register(Arc::new(BrowserClickTool::new(browser_manager.clone())));
        ctx.registry
            .register(Arc::new(BrowserTypeTool::new(browser_manager.clone())));
        ctx.registry
            .register(Arc::new(BrowserEvalTool::new(browser_manager.clone())));
        ctx.registry
            .register(Arc::new(BrowserContentTool::new(browser_manager.clone())));
        ctx.registry
            .register(Arc::new(BrowserWaitTool::new(browser_manager.clone())));
        ctx.registry
            .register(Arc::new(BrowserFindTool::new(browser_manager.clone())));
        ctx.registry
            .register(Arc::new(BrowserCloseTool::new(browser_manager)));
    }
    #[cfg(not(feature = "browser"))]
    fn register(&self, _ctx: &ModuleContext) {
        tracing::debug!("Browser module skipped (feature not enabled)");
    }
}

/// Meta tools: tool_manage, rsi_proposals
#[cfg(feature = "tools-meta")]
struct MetaModule;

#[cfg(feature = "tools-meta")]
impl ToolModule for MetaModule {
    fn id(&self) -> &str {
        "meta"
    }
    fn name(&self) -> &str {
        "Meta Tools"
    }
    fn description(&self) -> &str {
        "Tool management, RSI proposals, and system evolution"
    }
    fn register(&self, ctx: &ModuleContext) {
        use super::{rsi_proposals::RsiProposalsTool, tool_manage::ToolManageTool};

        let tools_toml_path = super::dynamic::DynamicToolLoader::default_path()
            .unwrap_or_else(|| std::path::PathBuf::from("tools.toml"));

        ctx.registry.register(Arc::new(ToolManageTool::new(
            ctx.registry.clone(),
            tools_toml_path.clone(),
        )));

        ctx.registry.register(Arc::new(RsiProposalsTool::new(
            ctx.registry.clone(),
            tools_toml_path,
            crate::config::opencrabs_home(),
        )));
    }
}

/// Dynamic tools from tools.toml
#[cfg(feature = "tools-dynamic")]
struct DynamicModule;

#[cfg(feature = "tools-dynamic")]
impl ToolModule for DynamicModule {
    fn id(&self) -> &str {
        "dynamic"
    }
    fn name(&self) -> &str {
        "Dynamic Tools"
    }
    fn description(&self) -> &str {
        "User-defined tools loaded from tools.toml at runtime"
    }
    fn register(&self, ctx: &ModuleContext) {
        let tools_toml_path = super::dynamic::DynamicToolLoader::default_path()
            .unwrap_or_else(|| std::path::PathBuf::from("tools.toml"));
        let count = super::dynamic::DynamicToolLoader::load(&tools_toml_path, &ctx.registry);
        if count > 0 {
            tracing::info!("Loaded {count} dynamic tool(s) from tools.toml");
        }
    }
}

// ---------------------------------------------------------------------------
// Module registry
// ---------------------------------------------------------------------------

/// Returns all available tool modules in registration order.
///
/// Only includes modules whose Cargo features are enabled. Modules disabled
/// at compile time are excluded from the binary entirely.
#[allow(clippy::vec_init_then_push)]
pub fn all_modules() -> Vec<Box<dyn ToolModule>> {
    let mut modules: Vec<Box<dyn ToolModule>> = Vec::new();
    #[cfg(feature = "tools-file-ops")]
    modules.push(Box::new(FileOpsModule));
    #[cfg(feature = "tools-search")]
    modules.push(Box::new(SearchModule));
    #[cfg(feature = "tools-workflow")]
    modules.push(Box::new(WorkflowModule));
    #[cfg(feature = "tools-multi-agent")]
    modules.push(Box::new(MultiAgentModule));
    #[cfg(feature = "tools-rsi")]
    modules.push(Box::new(RsiModule));
    #[cfg(feature = "tools-image")]
    modules.push(Box::new(ImageModule));
    #[cfg(feature = "tools-brain")]
    modules.push(Box::new(BrainModule));
    #[cfg(feature = "tools-channel-integrations")]
    modules.push(Box::new(ChannelIntegrationsModule));
    #[cfg(feature = "tools-browser")]
    modules.push(Box::new(BrowserModule));
    #[cfg(feature = "tools-meta")]
    modules.push(Box::new(MetaModule));
    #[cfg(feature = "tools-dynamic")]
    modules.push(Box::new(DynamicModule));
    modules
}

/// Returns the list of available module IDs for documentation/config help.
pub fn available_module_ids() -> Vec<String> {
    all_modules().iter().map(|m| m.id().to_string()).collect()
}

/// Register all enabled tools into the registry based on config.
///
/// This is the single entry point for tool registration, replacing the
/// duplicated registration code that was previously in ui.rs and commands.rs.
///
/// Special values in `disabled`:
/// - `"all"` — disables all modules (chatbot mode, no tools available)
pub fn register_enabled_tools(
    config: &Config,
    pool: &Pool,
    mode: RegistrationMode,
) -> Arc<ToolRegistry> {
    let registry = Arc::new(ToolRegistry::new());

    let disabled: HashSet<String> = config
        .tools
        .disabled
        .iter()
        .map(|s| s.to_lowercase())
        .collect();

    // Chatbot mode: disable all modules
    let disable_all = disabled.contains("all");

    #[cfg(feature = "tools-multi-agent")]
    let subagent_manager = Arc::new(SubAgentManager::new());
    #[cfg(feature = "tools-multi-agent")]
    let team_manager = Arc::new(TeamManager::new());

    let ctx = ModuleContext {
        registry: registry.clone(),
        config: config.clone(),
        pool: pool.clone(),
        mode,
        #[cfg(feature = "tools-multi-agent")]
        subagent_manager,
        #[cfg(feature = "tools-multi-agent")]
        team_manager,
    };

    let mut registered_count = 0usize;
    let mut skipped_count = 0usize;

    for module in all_modules() {
        if disable_all || disabled.contains(module.id()) {
            tracing::info!(
                "Skipping disabled tool module: {} ({})",
                module.id(),
                module.name()
            );
            skipped_count += 1;
            continue;
        }
        if !module.enabled_by_default() && !config.tools.enabled.contains(&module.id().to_string())
        {
            tracing::debug!(
                "Skipping opt-in tool module: {} ({})",
                module.id(),
                module.name()
            );
            skipped_count += 1;
            continue;
        }

        let before = registry.count();
        module.register(&ctx);
        let added = registry.count() - before;
        if added > 0 {
            tracing::info!(
                "Registered tool module: {} ({} tool(s))",
                module.name(),
                added
            );
        }
        registered_count += added;
    }

    tracing::info!(
        "Tool registration complete: {} tool(s) from {} module(s) ({} module(s) disabled)",
        registered_count,
        all_modules().len() - skipped_count,
        skipped_count
    );

    registry
}
