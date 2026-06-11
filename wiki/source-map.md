# Source Map

## Entry Points

| File | Responsibility |
|------|---------------|
| `src/main.rs` | Binary entry — clap parse, logging init, CLI dispatch |
| `src/lib.rs` | Crate root — module declarations, re-exports, version constants |
| `src/cli/commands.rs` | CLI subcommand dispatch (chat, run, agent, status, doctor, config, memory, session, db, cron, logs, service, daemon, completions, profile, init, onboard, channel, version) |

## Config

| File | Responsibility |
|------|---------------|
| `src/config/types.rs` | `Config` struct (126KB) — all TOML config sections |
| `src/config/mod.rs` | Module root |
| `src/config/crabrace.rs` | Provider registry via Crabrace |
| `src/config/secrets.rs` | SecretString (zeroize-on-drop) handling |
| `src/config/profile.rs` | Config profiles |
| `src/config/health.rs` | Config health checks |
| `src/config/update.rs` | Config update/refresh |
| `config.toml.example` | Example config |
| `keys.toml.example` | Example secrets/keys |
| `build_toggles.toml` | Build-time feature toggles |

## Database

| File | Responsibility |
|------|---------------|
| `src/db/database.rs` | SQLite pool init (deadpool-sqlite), connection management |
| `src/db/mod.rs` | Module root |
| `src/db/models.rs` | Data models |
| `src/db/retry.rs` | Retry logic |
| `src/db/repository/session.rs` | Session CRUD |
| `src/db/repository/message.rs` | Message CRUD |
| `src/db/repository/channel_message.rs` | Channel message persistence |
| `src/db/repository/cron_job.rs` | Cron job CRUD |
| `src/db/repository/cron_job_run.rs` | Cron job run log |
| `src/db/repository/feedback_ledger.rs` | Feedback records |
| `src/db/repository/plan.rs` | Plan persistence |
| `src/db/repository/tool_execution.rs` | Tool execution records |
| `src/db/repository/usage_ledger.rs` | Usage tracking |
| `src/db/repository/pending_request.rs` | Pending channel requests |
| `src/db/repository/file.rs` | File records |
| `src/db/repository/recent_paths.rs` | Recently accessed paths |

## LLM Providers

| File | Responsibility |
|------|---------------|
| `src/brain/provider/factory.rs` | Provider instantiation — 57KB, all LLM backend wiring |
| `src/brain/provider/trait.rs` | `Provider` trait definition |
| `src/brain/provider/anthropic.rs` | Anthropic Claude API |
| `src/brain/provider/gemini.rs` | Google Gemini API |
| `src/brain/provider/copilot.rs` | GitHub Copilot API |
| `src/brain/provider/qwen.rs` | Qwen API |
| `src/brain/provider/claude_cli.rs` | Claude CLI subprocess wrapper |
| `src/brain/provider/codex_cli.rs` | Codex CLI subprocess wrapper |
| `src/brain/provider/opencode_cli.rs` | OpenCode CLI subprocess wrapper |
| `src/brain/provider/custom_openai_compatible.rs` | Any OpenAI-compatible endpoint (Ollama, LM Studio, vLLM, etc.) |
| `src/brain/provider/fallback.rs` | Provider fallback chain |
| `src/brain/provider/retry.rs` | Provider retry logic |
| `src/brain/provider/rate_limiter.rs` | Rate limiting |
| `src/brain/provider/types.rs` | Provider types (`LLMRequest`, `LLMResponse`, `StreamEvent`) |
| `src/brain/provider/error.rs` | Provider error types |
| `src/brain/provider/model_fetch.rs` | Model listing fetch |
| `src/brain/provider/rig_adapter.rs` | rig-core adapter |
| `src/brain/provider/nonstream_compat.rs` | Non-streaming compat wrapper |
| `src/brain/provider/streaming_utils.rs` | Streaming utilities |
| `src/brain/provider/json_repair.rs` | JSON repair utilities |
| `src/brain/provider/bare_tool_call_extractor.rs` | Bare tool call extraction |
| `src/brain/provider/codex_oauth.rs` | Codex OAuth flow |
| `src/brain/provider/placeholder.rs` | Placeholder provider |

## Agent Service

| File | Responsibility |
|------|---------------|
| `src/brain/agent/service/mod.rs` | Agent service root |
| `src/brain/agent/service/tool_loop.rs` | Core tool execution loop — 253KB |
| `src/brain/agent/service/context.rs` | Context management (31KB) |
| `src/brain/agent/service/compaction.rs` | Context compaction (65% soft, 90% hard) |
| `src/brain/agent/service/gaslighting.rs` | System prompt gaslighting |
| `src/brain/agent/service/phantom.rs` | Phantom mode |
| `src/brain/agent/service/helpers.rs` | Agent service helpers (61KB) |
| `src/brain/agent/service/messaging.rs` | Agent message handling |
| `src/brain/agent/service/builder.rs` | Agent builder |
| `src/brain/agent/service/truncation.rs` | Message truncation |
| `src/brain/agent/service/types.rs` | Service types |
| `src/brain/agent/service/feedback.rs` | Feedback processing |
| `src/brain/agent/service/compaction_prompts.rs` | Compaction prompt templates |
| `src/brain/agent/service/phantom_lang/` | Phantom language (internal DSL) |

## Tools

| File | Responsibility |
|------|---------------|
| `src/brain/tools/registry.rs` | Tool registration & lookup |
| `src/brain/tools/mod.rs` | Module root — tool declarations |
| `src/brain/tools/trait.rs` | `Tool` trait definition |
| `src/brain/tools/bash.rs` | Shell execution |
| `src/brain/tools/edit.rs` | File editing |
| `src/brain/tools/read.rs` | File reading |
| `src/brain/tools/write.rs` | File writing |
| `src/brain/tools/glob.rs` | Glob pattern search |
| `src/brain/tools/grep.rs` | Content search |
| `src/brain/tools/ls.rs` | Directory listing |
| `src/brain/tools/web_search.rs` | Web search |
| `src/brain/tools/exa_search.rs` | Exa search |
| `src/brain/tools/brave_search.rs` | Brave search |
| `src/brain/tools/memory_search.rs` | Memory search |
| `src/brain/tools/session_search.rs` | Session search |
| `src/brain/tools/channel_search.rs` | Channel search |
| `src/brain/tools/http.rs` | HTTP requests |
| `src/brain/tools/code_exec.rs` | Code execution |
| `src/brain/tools/doc_parser.rs` | Document parsing |
| `src/brain/tools/task.rs` | Task management |
| `src/brain/tools/plan_tool.rs` | Planning |
| `src/brain/tools/notebook.rs` | Notebook editing |
| `src/brain/tools/context.rs` | Session context |
| `src/brain/tools/config_tool.rs` | Config management |
| `src/brain/tools/follow_up_question.rs` | Follow-up questions |
| `src/brain/tools/cron_manage.rs` | Cron management |
| `src/brain/tools/generate_image.rs` | Image generation |
| `src/brain/tools/analyze_image.rs` | Image analysis |
| `src/brain/tools/analyze_video.rs` | Video analysis |
| `src/brain/tools/slash_command.rs` | Slash commands |
| `src/brain/tools/rename_session.rs` | Session renaming |
| `src/brain/tools/load_brain_file.rs` | Load brain file |
| `src/brain/tools/write_stemcell_file.rs` | Write stemcell file |
| `src/brain/tools/a2a_send.rs` | A2A send |
| `src/brain/tools/browser/manager.rs` | CDP browser manager (48KB) |
| `src/brain/tools/browser/navigate.rs` | Browser navigation |
| `src/brain/tools/browser/screenshot.rs` | Browser screenshot |
| `src/brain/tools/browser/click.rs` | Browser click |
| `src/brain/tools/browser/type_text.rs` | Browser text input |
| `src/brain/tools/browser/eval.rs` | Browser JS eval |
| `src/brain/tools/browser/content.rs` | Browser content extraction |
| `src/brain/tools/browser/wait.rs` | Browser wait |
| `src/brain/tools/browser/find.rs` | Browser find |
| `src/brain/tools/browser/close.rs` | Browser close |
| `src/brain/tools/subagent/spawn.rs` | Subagent spawning |
| `src/brain/tools/subagent/wait.rs` | Subagent wait |
| `src/brain/tools/subagent/send_input.rs` | Subagent input |
| `src/brain/tools/subagent/close.rs` | Subagent close |
| `src/brain/tools/subagent/resume.rs` | Subagent resume |
| `src/brain/tools/subagent/team/` | Team management (create, delete, broadcast) |
| `src/brain/tools/feedback_record.rs` | RSI feedback recording |
| `src/brain/tools/feedback_analyze.rs` | RSI feedback analysis |
| `src/brain/tools/self_improve.rs` | RSI self-improvement |
| `src/brain/tools/rsi_propose.rs` | RSI proposal |
| `src/brain/tools/rsi_proposals.rs` | RSI proposals tool |
| `src/brain/tools/tool_manage.rs` | Tool management |
| `src/brain/tools/rebuild.rs` | Self-rebuild |
| `src/brain/tools/evolve.rs` | Self-evolution |
| `src/brain/tools/dynamic/loader.rs` | Dynamic tool loading |
| `src/brain/tools/dynamic/tool.rs` | Dynamic tool execution |
| `src/brain/tools/hashline/` | Hashline editing system |
| `src/brain/tools/fuzzy.rs` | Fuzzy search |
| `src/brain/tools/brain_file_safety.rs` | Brain file safety checks |
| `src/brain/tools/provider_vision.rs` | Provider vision integration |
| `src/brain/tools/modules.rs` | Module listing tool |
| `src/brain/tools/error.rs` | Tool error types |

## Channels

| File | Responsibility |
|------|---------------|
| `src/channels/factory.rs` | Channel agent factory |
| `src/channels/mod.rs` | Module root |
| `src/channels/commands.rs` | Channel commands (42KB) |
| `src/channels/session_init.rs` | Channel session init |
| `src/channels/session_resolve.rs` | Channel session resolution |
| `src/channels/greeting.rs` | Channel greeting messages |
| `src/channels/tests.rs` | Channel tests |
| `src/channels/gateway/` | Unified gateway bus: surfaces, envelope, registry, shared services |
| `src/channels/tui_surface.rs` | TUI surface adapter |
| `src/channels/telegram_surface.rs` | Telegram surface adapter |
| `src/channels/discord_surface.rs` | Discord surface adapter |
| `src/channels/slack_surface.rs` | Slack surface adapter |
| `src/channels/whatsapp_surface.rs` | WhatsApp surface adapter |
| `src/channels/trello_surface.rs` | Trello surface adapter |
| `src/channels/telegram/` | Telegram bot handlers |
| `src/channels/discord/` | Discord bot handlers |
| `src/channels/slack/` | Slack bot handlers |
| `src/channels/whatsapp/` | WhatsApp handlers |
| `src/channels/trello/` | Trello integration |
| `src/channels/voice/` | STT/TTS voice service |

## TUI

| File | Responsibility |
|------|---------------|
| `src/tui/runner.rs` | TUI main loop (crossterm event loop) |
| `src/tui/app/mod.rs` | TUI app state & dispatch |
| `src/tui/render/mod.rs` | TUI rendering root |
| `src/tui/render/chat.rs` | Chat render |
| `src/tui/render/dialogs.rs` | Dialog render (57KB) |
| `src/tui/render/help.rs` | Help render |
| `src/tui/render/input.rs` | Input render (37KB) |
| `src/tui/render/panes.rs` | Pane render |
| `src/tui/render/sessions.rs` | Session list render |
| `src/tui/render/tools.rs` | Tools panel render |
| `src/tui/render/plan_widget.rs` | Plan widget |
| `src/tui/render/plan_window.rs` | Plan window |
| `src/tui/render/palette.rs` | Color palette |
| `src/tui/render/utils.rs` | Render utilities |
| `src/tui/onboarding/` | Onboarding wizard (5 screen images in `src/assets/`) |
| `src/tui/pane/` | Split pane system |
| `src/tui/events.rs` | Event handling |
| `src/tui/markdown.rs` | Markdown rendering |
| `src/tui/highlight.rs` | Syntax highlighting |
| `src/tui/plan.rs` | Plan rendering |
| `src/tui/error.rs` | TUI error handling |
| `src/tui/onboarding_render.rs` | Onboarding screen render (109KB) |
| `src/tui/prompt_analyzer.rs` | Prompt analysis |
| `src/tui/provider_selector.rs` | Provider selector UI |

## A2A

| File | Responsibility |
|------|---------------|
| `src/a2a/server.rs` | Axum HTTP server (JSON-RPC 2.0) |
| `src/a2a/types.rs` | A2A protocol types |
| `src/a2a/handler/` | Request handlers |
| `src/a2a/debate.rs` | Multi-agent debate |
| `src/a2a/agent_card.rs` | Agent card definition |
| `src/a2a/persistence.rs` | A2A task persistence |

## Cron

| File | Responsibility |
|------|---------------|
| `src/cron/scheduler.rs` | Cron scheduler loop |

## Memory

| File | Responsibility |
|------|---------------|
| `src/memory/mod.rs` | Memory module root |
| `src/memory/search.rs` | Hybrid search (FTS5 + vector, RRF merge) |
| `src/memory/embedding.rs` | Vector embedding generation |
| `src/memory/index.rs` | FTS5 index management |
| `src/memory/store.rs` | Memory storage |

## RSI (Recursive Self-Improvement)

| File | Responsibility |
|------|---------------|
| `src/brain/rsi.rs` | Core RSI engine (42KB) |
| `src/brain/rsi_proposals.rs` | RSI proposal generation |
| `src/brain/rsi_pruned.rs` | Pruned RSI proposals |
| `src/brain/rsi_subsystem.rs` | RSI subsystem analysis |
| `src/brain/rsi_git_history.rs` | Git history for RSI |
| `src/brain/rsi_sync.rs` | RSI sync logic |
| `src/brain/mission_control/mod.rs` | Mission control root |
| `src/brain/mission_control/activity_service.rs` | Activity feed |
| `src/brain/mission_control/inbox_service.rs` | Inbox service |
| `src/brain/mission_control/schedule_service.rs` | Cron schedule service |
| `src/brain/mission_control/types.rs` | Mission control types |

## Other

| File | Responsibility |
|------|---------------|
| `src/services/` | Business logic services (session, message, file, plan, context) |
| `src/error/` | Typed errors (`StemCellError`, `ErrorCode`) |
| `src/logging/logger.rs` | Tracing logger init |
| `src/startup/` | Startup job runner |
| `src/rtk/` | Rust Token Killer integration |
| `src/usage/` | Usage analytics (dashboard, cards, categorizer, pricing, data) |
| `src/utils/` | Shared utilities (approval, config_watcher, fd_suppress, file_extract, git_branch, image, install, pdf_vision, providers, retry, sanitize, slack_fmt, string, text_complete, tool_context) |
| `src/patches/wacore-binary/` | Patched WhatsApp library binary |
| `src/scripts/` | Build/dev scripts (install.sh, setup.sh, tool_features.py) |
| `src/assets/` | Icons and screenshots |

## Tests

| Path | Count |
|------|-------|
| `src/tests/` | 228 test files, ~2900 tests |
| `src/benches/database.rs` | Database benchmark |
| `src/benches/memory.rs` | Memory benchmark |

## Generated Files

- `target/` — build artifacts (gitignored)
- `Cargo.lock` — dependency lockfile

## External Boundaries

- **LLM APIs**: Anthropic, Google Gemini, GitHub Copilot, Qwen, OpenAI-compatible endpoints
- **Messaging APIs**: Telegram Bot API, Discord Gateway, Slack Socket Mode, WhatsApp Web, Trello REST
- **Browser**: CDP protocol via chromey (Chrome DevTools Protocol)
- **System**: Shell execution, file system, signal handling
- **Network**: HTTP server (A2A), outbound HTTP (providers, tools)
