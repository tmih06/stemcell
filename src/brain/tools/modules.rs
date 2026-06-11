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
use super::subagent::{SubAgentManager, TeamManager};

/// Runtime-only dependencies some tool modules need at registration time.
///
/// These are optional so non-interactive modes can still use the same module
/// registrar without fabricating TUI/channel state they do not have.
#[derive(Clone, Default)]
pub struct RuntimeToolContext {
    pub progress_callback: Option<crate::brain::agent::ProgressCallback>,
    pub channel_factory: Option<Arc<crate::channels::ChannelFactory>>,
    #[cfg(feature = "telegram")]
    pub telegram_state: Option<Arc<crate::channels::telegram::TelegramState>>,
    #[cfg(feature = "whatsapp")]
    pub whatsapp_state: Option<Arc<crate::channels::whatsapp::WhatsAppState>>,
    #[cfg(feature = "discord")]
    pub discord_state: Option<Arc<crate::channels::discord::DiscordState>>,
    #[cfg(feature = "slack")]
    pub slack_state: Option<Arc<crate::channels::slack::SlackState>>,
    #[cfg(feature = "trello")]
    pub trello_state: Option<Arc<crate::channels::trello::TrelloState>>,
}

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
    pub runtime: RuntimeToolContext,
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
    pub subagent_manager: Arc<SubAgentManager>,
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
    pub team_manager: Arc<TeamManager>,
}

impl ModuleContext {
    /// Registers a tool into the registry, but skips it if the tool's name
    /// is listed in the `tools.disabled` config array. This allows granular
    /// tool-by-tool toggling instead of just module-level toggling.
    pub fn register(&self, tool: Arc<dyn super::r#trait::Tool>) {
        let name = tool.name();

        let disabled: HashSet<String> = self
            .config
            .tools
            .disabled
            .iter()
            .map(|s| s.to_lowercase())
            .collect();

        if disabled.contains("all") || disabled.contains(&name.to_lowercase()) {
            tracing::info!("Skipping disabled individual tool: {}", name);
            return;
        }

        self.registry.register(tool);
    }
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
#[cfg(any(
    feature = "tool-read",
    feature = "tool-write",
    feature = "tool-edit",
    feature = "tool-hashline-edit",
    feature = "tool-bash",
    feature = "tool-ls",
    feature = "tool-glob",
    feature = "tool-grep",
    feature = "tools-file-ops"
))]
struct FileOpsModule;

#[cfg(any(
    feature = "tool-read",
    feature = "tool-write",
    feature = "tool-edit",
    feature = "tool-hashline-edit",
    feature = "tool-bash",
    feature = "tool-ls",
    feature = "tool-glob",
    feature = "tool-grep",
    feature = "tools-file-ops"
))]
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
        #[cfg(feature = "tool-read")]
        ctx.register(Arc::new(super::read::ReadTool));
        #[cfg(feature = "tool-write")]
        ctx.register(Arc::new(super::write::WriteTool));
        #[cfg(feature = "tool-edit")]
        ctx.register(Arc::new(super::edit::EditTool));
        #[cfg(feature = "tool-hashline-edit")]
        ctx.register(Arc::new(super::hashline::HashlineEditTool));
        #[cfg(feature = "tool-bash")]
        ctx.register(Arc::new(super::bash::BashTool));
        #[cfg(feature = "tool-ls")]
        ctx.register(Arc::new(super::ls::LsTool));
        #[cfg(feature = "tool-glob")]
        ctx.register(Arc::new(super::glob::GlobTool));
        #[cfg(feature = "tool-grep")]
        ctx.register(Arc::new(super::grep::GrepTool));
    }
}

/// Search & memory: web_search, exa_search, brave_search, memory_search,
/// session_search, channel_search
#[cfg(any(
    feature = "tool-web-search",
    feature = "tool-memory-search",
    feature = "tool-session-search",
    feature = "tool-channel-search",
    feature = "tool-exa-search",
    feature = "tool-brave-search",
    feature = "tools-search"
))]
struct SearchModule;

#[cfg(any(
    feature = "tool-web-search",
    feature = "tool-memory-search",
    feature = "tool-session-search",
    feature = "tool-channel-search",
    feature = "tool-exa-search",
    feature = "tool-brave-search",
    feature = "tools-search"
))]
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
        #[cfg(feature = "tool-web-search")]
        ctx.register(Arc::new(super::web_search::WebSearchTool));

        #[cfg(feature = "tool-memory-search")]
        ctx.register(Arc::new(super::memory_search::MemorySearchTool));

        #[cfg(feature = "tool-session-search")]
        ctx.register(Arc::new(super::session_search::SessionSearchTool::new(
            ctx.pool.clone(),
        )));

        #[cfg(feature = "tool-exa-search")]
        {
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
            ctx.register(Arc::new(super::exa_search::ExaSearchTool::new(exa_key)));
            tracing::info!("Registered EXA search tool (mode: {})", exa_mode);
        }

        #[cfg(feature = "tool-brave-search")]
        {
            if let Some(brave_cfg) = ctx
                .config
                .providers
                .web_search
                .as_ref()
                .and_then(|ws| ws.brave.as_ref())
                && brave_cfg.enabled
                && let Some(brave_key) = brave_cfg.api_key.clone()
            {
                ctx.register(Arc::new(super::brave_search::BraveSearchTool::new(
                    brave_key,
                )));
                tracing::info!("Registered Brave search tool");
            }
        }

        #[cfg(feature = "tool-channel-search")]
        if ctx.mode == RegistrationMode::Full {
            ctx.register(Arc::new(super::channel_search::ChannelSearchTool::new(
                crate::db::ChannelMessageRepository::new(ctx.pool.clone()),
            )));
        }
    }
}

/// Workflow & integration: task, plan, context, config, cron, notebook,
/// doc_parser, http, code_exec, follow_up_question
#[cfg(any(
    feature = "tool-task-manager",
    feature = "tool-session-context",
    feature = "tool-http-request",
    feature = "tool-plan",
    feature = "tool-execute-code",
    feature = "tool-notebook-edit",
    feature = "tool-parse-document",
    feature = "tool-config-manager",
    feature = "tool-follow-up-question",
    feature = "tool-cron-manage"
))]
struct WorkflowModule;

#[cfg(any(
    feature = "tool-task-manager",
    feature = "tool-session-context",
    feature = "tool-http-request",
    feature = "tool-plan",
    feature = "tool-execute-code",
    feature = "tool-notebook-edit",
    feature = "tool-parse-document",
    feature = "tool-config-manager",
    feature = "tool-follow-up-question",
    feature = "tool-cron-manage"
))]
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
        #[cfg(feature = "tool-task-manager")]
        ctx.register(Arc::new(TaskTool));
        #[cfg(feature = "tool-session-context")]
        ctx.register(Arc::new(ContextTool));
        #[cfg(feature = "tool-http-request")]
        ctx.register(Arc::new(HttpClientTool));
        #[cfg(feature = "tool-plan")]
        ctx.register(Arc::new(PlanTool));
        #[cfg(feature = "tool-execute-code")]
        ctx.register(Arc::new(CodeExecTool));
        #[cfg(feature = "tool-notebook-edit")]
        ctx.register(Arc::new(NotebookEditTool));
        #[cfg(feature = "tool-parse-document")]
        ctx.register(Arc::new(DocParserTool));
        #[cfg(feature = "tool-config-manager")]
        ctx.register(Arc::new(ConfigTool));
        #[cfg(feature = "tool-follow-up-question")]
        ctx.register(Arc::new(FollowUpQuestionTool));

        // Cron job management — Full mode only
        #[cfg(feature = "tool-cron-manage")]
        if ctx.mode == RegistrationMode::Full {
            use super::cron_manage::CronManageTool;
            ctx.register(Arc::new(CronManageTool::new(
                crate::db::CronJobRepository::new(ctx.pool.clone()),
            )));
        }
    }
}

/// Multi-agent orchestration: spawn, wait, send_input, close, resume + team tools
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
struct MultiAgentModule;

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

        #[cfg(feature = "tool-spawn-agent")]
        ctx.register(Arc::new(SpawnAgentTool::new(
            ctx.subagent_manager.clone(),
            ctx.registry.clone(),
        )));
        #[cfg(feature = "tool-wait-agent")]
        ctx.register(Arc::new(WaitAgentTool::new(ctx.subagent_manager.clone())));
        #[cfg(feature = "tool-send-input")]
        ctx.register(Arc::new(SendInputTool::new(ctx.subagent_manager.clone())));
        #[cfg(feature = "tool-close-agent")]
        ctx.register(Arc::new(CloseAgentTool::new(ctx.subagent_manager.clone())));
        #[cfg(feature = "tool-resume-agent")]
        ctx.register(Arc::new(ResumeAgentTool::new(
            ctx.subagent_manager.clone(),
            ctx.registry.clone(),
        )));

        #[cfg(feature = "tool-team-create")]
        ctx.register(Arc::new(TeamCreateTool::new(
            ctx.subagent_manager.clone(),
            ctx.team_manager.clone(),
            ctx.registry.clone(),
        )));
        #[cfg(feature = "tool-team-delete")]
        ctx.register(Arc::new(TeamDeleteTool::new(
            ctx.subagent_manager.clone(),
            ctx.team_manager.clone(),
        )));
        #[cfg(feature = "tool-team-broadcast")]
        ctx.register(Arc::new(TeamBroadcastTool::new(
            ctx.subagent_manager.clone(),
            ctx.team_manager.clone(),
        )));
    }
}

/// RSI (Recursive Self-Improvement): feedback_record, feedback_analyze, self_improve
#[cfg(any(
    feature = "tool-feedback-record",
    feature = "tool-feedback-analyze",
    feature = "tool-self-improve",
    feature = "tool-rsi-propose"
))]
struct RsiModule;

#[cfg(any(
    feature = "tool-feedback-record",
    feature = "tool-feedback-analyze",
    feature = "tool-self-improve",
    feature = "tool-rsi-propose"
))]
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
            rsi_propose::RsiProposeTool, self_improve::SelfImproveTool,
        };
        #[cfg(feature = "tool-feedback-record")]
        ctx.register(Arc::new(FeedbackRecordTool));
        #[cfg(feature = "tool-feedback-analyze")]
        ctx.register(Arc::new(FeedbackAnalyzeTool));
        #[cfg(feature = "tool-self-improve")]
        ctx.register(Arc::new(SelfImproveTool));
        #[cfg(feature = "tool-rsi-propose")]
        ctx.register(Arc::new(RsiProposeTool));
    }
}

/// Image & vision: generate_image, analyze_image, provider_vision, analyze_video
#[cfg(any(
    feature = "tool-generate-image",
    feature = "tool-analyze-image",
    feature = "tool-analyze-video"
))]
struct ImageModule;

#[cfg(any(
    feature = "tool-generate-image",
    feature = "tool-analyze-image",
    feature = "tool-analyze-video"
))]
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
        #[cfg(feature = "tool-generate-image")]
        if let Some(tool) = GenerateImageTool::from_config(&ctx.config) {
            ctx.register(Arc::new(tool));
            tracing::info!("Registered generate_image tool");
        }

        // Vision: provider.vision_model takes priority over image.vision (Gemini)
        #[cfg(feature = "tool-analyze-image")]
        if let Some((api_key, base_url, vision_model)) =
            crate::brain::provider::factory::active_provider_vision(&ctx.config)
        {
            ctx.register(Arc::new(ProviderVisionTool::new(
                api_key,
                base_url,
                vision_model,
            )));
            tracing::info!("Registered analyze_image tool (provider vision model)");
        } else if ctx.config.image.vision.enabled
            && let Some(ref key) = ctx.config.image.vision.api_key
        {
            ctx.register(Arc::new(AnalyzeImageTool::new(
                key.clone(),
                ctx.config.image.vision.model.clone(),
            )));
            tracing::info!("Registered analyze_image tool (Gemini)");
        }

        // Video vision — Gemini-native multimodal video understanding
        #[cfg(feature = "tool-analyze-video")]
        if ctx.config.image.vision.enabled
            && let Some(ref key) = ctx.config.image.vision.api_key
            && !key.is_empty()
        {
            ctx.register(Arc::new(AnalyzeVideoTool::new(
                key.clone(),
                ctx.config.image.vision.model.clone(),
            )));
            tracing::info!("Registered analyze_video tool (Gemini)");
        }
    }
}

/// Brain & session management: load_brain_file, write_stemcell_file,
/// rename_session, slash_command, a2a_send
#[cfg(any(
    feature = "tool-slash-command",
    feature = "tool-rename-session",
    feature = "tool-load-brain-file",
    feature = "tool-write-stemcell-file",
    feature = "tool-a2a-send"
))]
struct BrainModule;

#[cfg(any(
    feature = "tool-slash-command",
    feature = "tool-rename-session",
    feature = "tool-load-brain-file",
    feature = "tool-write-stemcell-file",
    feature = "tool-a2a-send"
))]
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
            write_stemcell_file::WriteStemCellFileTool,
        };

        #[cfg(feature = "tool-slash-command")]
        ctx.register(Arc::new(SlashCommandTool));
        #[cfg(feature = "tool-rename-session")]
        ctx.register(Arc::new(RenameSessionTool));

        // Full mode only: brain file loader, stemcell file writer, A2A
        if ctx.mode == RegistrationMode::Full {
            #[cfg(feature = "tool-load-brain-file")]
            ctx.register(Arc::new(LoadBrainFileTool));
            #[cfg(feature = "tool-write-stemcell-file")]
            ctx.register(Arc::new(WriteStemCellFileTool));
            #[cfg(feature = "tool-a2a-send")]
            ctx.register(Arc::new(A2aSendTool::new()));
        }
    }
}

/// Channel integrations: discord, slack, telegram, trello, whatsapp
/// (feature-gated)
#[cfg(any(
    feature = "tool-telegram-connect",
    feature = "tool-telegram-send",
    feature = "tool-whatsapp-connect",
    feature = "tool-whatsapp-send",
    feature = "tool-discord-connect",
    feature = "tool-discord-send",
    feature = "tool-slack-connect",
    feature = "tool-slack-send",
    feature = "tool-trello-connect",
    feature = "tool-trello-send"
))]
struct ChannelIntegrationsModule;

#[cfg(any(
    feature = "tool-telegram-connect",
    feature = "tool-telegram-send",
    feature = "tool-whatsapp-connect",
    feature = "tool-whatsapp-send",
    feature = "tool-discord-connect",
    feature = "tool-discord-send",
    feature = "tool-slack-connect",
    feature = "tool-slack-send",
    feature = "tool-trello-connect",
    feature = "tool-trello-send"
))]
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
    fn register(&self, ctx: &ModuleContext) {
        if ctx.mode != RegistrationMode::Full {
            tracing::debug!("Channel integrations skipped outside full registration mode");
            return;
        }

        let Some(channel_factory) = ctx.runtime.channel_factory.clone() else {
            tracing::debug!("Channel integrations skipped (channel factory unavailable)");
            return;
        };

        #[cfg(any(feature = "tool-telegram-connect", feature = "tool-telegram-send"))]
        if let Some(state) = ctx.runtime.telegram_state.clone() {
            #[cfg(feature = "tool-telegram-connect")]
            ctx.register(Arc::new(super::telegram_connect::TelegramConnectTool::new(
                channel_factory.clone(),
                state.clone(),
            )));
            #[cfg(feature = "tool-telegram-send")]
            ctx.register(Arc::new(super::telegram_send::TelegramSendTool::new(state)));
        }

        #[cfg(any(feature = "tool-whatsapp-connect", feature = "tool-whatsapp-send"))]
        if let Some(state) = ctx.runtime.whatsapp_state.clone() {
            #[cfg(feature = "tool-whatsapp-connect")]
            ctx.register(Arc::new(super::whatsapp_connect::WhatsAppConnectTool::new(
                ctx.runtime.progress_callback.clone(),
                state.clone(),
            )));
            #[cfg(feature = "tool-whatsapp-send")]
            ctx.register(Arc::new(super::whatsapp_send::WhatsAppSendTool::new(
                state,
                channel_factory.config_rx(),
            )));
        }

        #[cfg(any(feature = "tool-discord-connect", feature = "tool-discord-send"))]
        if let Some(state) = ctx.runtime.discord_state.clone() {
            #[cfg(feature = "tool-discord-connect")]
            ctx.register(Arc::new(super::discord_connect::DiscordConnectTool::new(
                channel_factory.clone(),
                state.clone(),
            )));
            #[cfg(feature = "tool-discord-send")]
            ctx.register(Arc::new(super::discord_send::DiscordSendTool::new(state)));
        }

        #[cfg(any(feature = "tool-slack-connect", feature = "tool-slack-send"))]
        if let Some(state) = ctx.runtime.slack_state.clone() {
            #[cfg(feature = "tool-slack-connect")]
            ctx.register(Arc::new(super::slack_connect::SlackConnectTool::new(
                channel_factory.clone(),
                state.clone(),
            )));
            #[cfg(feature = "tool-slack-send")]
            ctx.register(Arc::new(super::slack_send::SlackSendTool::new(state)));
        }

        #[cfg(any(feature = "tool-trello-connect", feature = "tool-trello-send"))]
        if let Some(state) = ctx.runtime.trello_state.clone() {
            #[cfg(feature = "tool-trello-connect")]
            ctx.register(Arc::new(super::trello_connect::TrelloConnectTool::new(
                channel_factory.clone(),
                state.clone(),
            )));
            #[cfg(feature = "tool-trello-send")]
            ctx.register(Arc::new(super::trello_send::TrelloSendTool::new(state)));
        }
    }
}

/// Browser automation tools (feature-gated)
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
struct BrowserModule;

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
        #[cfg(feature = "tool-browser-navigate")]
        ctx.register(Arc::new(BrowserNavigateTool::new(browser_manager.clone())));
        #[cfg(feature = "tool-browser-screenshot")]
        ctx.register(Arc::new(BrowserScreenshotTool::new(
            browser_manager.clone(),
        )));
        #[cfg(feature = "tool-browser-click")]
        ctx.register(Arc::new(BrowserClickTool::new(browser_manager.clone())));
        #[cfg(feature = "tool-browser-type")]
        ctx.register(Arc::new(BrowserTypeTool::new(browser_manager.clone())));
        #[cfg(feature = "tool-browser-eval")]
        ctx.register(Arc::new(BrowserEvalTool::new(browser_manager.clone())));
        #[cfg(feature = "tool-browser-content")]
        ctx.register(Arc::new(BrowserContentTool::new(browser_manager.clone())));
        #[cfg(feature = "tool-browser-wait")]
        ctx.register(Arc::new(BrowserWaitTool::new(browser_manager.clone())));
        #[cfg(feature = "tool-browser-find")]
        ctx.register(Arc::new(BrowserFindTool::new(browser_manager.clone())));
        #[cfg(feature = "tool-browser-close")]
        ctx.register(Arc::new(BrowserCloseTool::new(browser_manager)));
    }
    #[cfg(not(feature = "browser"))]
    fn register(&self, _ctx: &ModuleContext) {
        tracing::debug!("Browser module skipped (feature not enabled)");
    }
}

/// Meta tools: tool_manage, rsi_proposals
#[cfg(any(
    feature = "tool-rebuild",
    feature = "tool-evolve",
    feature = "tool-tool-manage",
    feature = "tool-rsi-proposals"
))]
struct MetaModule;

#[cfg(any(
    feature = "tool-rebuild",
    feature = "tool-evolve",
    feature = "tool-tool-manage",
    feature = "tool-rsi-proposals"
))]
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

        if ctx.mode == RegistrationMode::Full {
            #[cfg(feature = "tool-evolve")]
            ctx.register(Arc::new(super::evolve::EvolveTool::new(
                ctx.runtime.progress_callback.clone(),
            )));
            #[cfg(feature = "tool-rebuild")]
            if ctx.runtime.progress_callback.is_some() {
                ctx.register(Arc::new(super::rebuild::RebuildTool::new(
                    ctx.runtime.progress_callback.clone(),
                )));
            }
        }

        #[cfg(feature = "tool-tool-manage")]
        ctx.register(Arc::new(ToolManageTool::new(
            ctx.registry.clone(),
            tools_toml_path.clone(),
        )));

        #[cfg(feature = "tool-rsi-proposals")]
        ctx.register(Arc::new(RsiProposalsTool::new(
            ctx.registry.clone(),
            tools_toml_path,
            crate::config::stemcell_home(),
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
    #[cfg(any(
        feature = "tool-read",
        feature = "tool-write",
        feature = "tool-edit",
        feature = "tool-hashline-edit",
        feature = "tool-bash",
        feature = "tool-ls",
        feature = "tool-glob",
        feature = "tool-grep",
        feature = "tools-file-ops"
    ))]
    modules.push(Box::new(FileOpsModule));
    #[cfg(any(
        feature = "tool-web-search",
        feature = "tool-memory-search",
        feature = "tool-session-search",
        feature = "tool-channel-search",
        feature = "tool-exa-search",
        feature = "tool-brave-search",
        feature = "tools-search"
    ))]
    modules.push(Box::new(SearchModule));
    #[cfg(any(
        feature = "tool-task-manager",
        feature = "tool-session-context",
        feature = "tool-http-request",
        feature = "tool-plan",
        feature = "tool-execute-code",
        feature = "tool-notebook-edit",
        feature = "tool-parse-document",
        feature = "tool-config-manager",
        feature = "tool-follow-up-question",
        feature = "tool-cron-manage"
    ))]
    modules.push(Box::new(WorkflowModule));
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
    modules.push(Box::new(MultiAgentModule));
    #[cfg(any(
        feature = "tool-feedback-record",
        feature = "tool-feedback-analyze",
        feature = "tool-self-improve",
        feature = "tool-rsi-propose"
    ))]
    modules.push(Box::new(RsiModule));
    #[cfg(any(
        feature = "tool-generate-image",
        feature = "tool-analyze-image",
        feature = "tool-analyze-video"
    ))]
    modules.push(Box::new(ImageModule));
    #[cfg(any(
        feature = "tool-slash-command",
        feature = "tool-rename-session",
        feature = "tool-load-brain-file",
        feature = "tool-write-stemcell-file",
        feature = "tool-a2a-send"
    ))]
    modules.push(Box::new(BrainModule));
    #[cfg(any(
        feature = "tool-telegram-connect",
        feature = "tool-telegram-send",
        feature = "tool-whatsapp-connect",
        feature = "tool-whatsapp-send",
        feature = "tool-discord-connect",
        feature = "tool-discord-send",
        feature = "tool-slack-connect",
        feature = "tool-slack-send",
        feature = "tool-trello-connect",
        feature = "tool-trello-send"
    ))]
    modules.push(Box::new(ChannelIntegrationsModule));
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
    modules.push(Box::new(BrowserModule));
    #[cfg(any(
        feature = "tool-rebuild",
        feature = "tool-evolve",
        feature = "tool-tool-manage",
        feature = "tool-rsi-proposals"
    ))]
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
    register_enabled_tools_with_runtime(config, pool, mode, RuntimeToolContext::default())
}

/// Register all enabled tools into the registry based on config plus any
/// runtime-only dependencies needed by certain modules.
pub fn register_enabled_tools_with_runtime(
    config: &Config,
    pool: &Pool,
    mode: RegistrationMode,
    runtime: RuntimeToolContext,
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
    let subagent_manager = Arc::new(SubAgentManager::new());
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
    let team_manager = Arc::new(TeamManager::new());

    let ctx = ModuleContext {
        registry: registry.clone(),
        config: config.clone(),
        pool: pool.clone(),
        mode,
        runtime,
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
        subagent_manager,
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
