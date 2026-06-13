# Brain Source Map

## Provider Layer (`src/brain/provider/`)

| File | Role |
|---|---|
| `trait.rs` | `Provider` trait — `complete()`, `stream()`, `supports_tools()`, `supports_streaming()` |
| `types.rs` | `LLMRequest`, `LLMResponse`, `Message`, `ContentBlock`, `Role`, `StopReason`, `TokenUsage`, `StreamEvent` |
| `factory.rs` | `create_provider()`, `create_provider_by_name()` — config-driven provider instantiation with registry pattern (~1500 lines) |
| `error.rs` | `ProviderError` enum |
| `mod.rs` | Module re-exports; `which_binary()` helper for CLI wrappers |
| `anthropic.rs` | Anthropic Claude API native integration |
| `gemini.rs` | Google Gemini API native integration |
| `copilot.rs` | GitHub Copilot provider |
| `qwen.rs` | Qwen native provider with `looks_like_qwen_target()`, `qwen_body_transform()`, `qwen_extra_headers()` |
| `custom_openai_compatible.rs` | `OpenAIProvider` — works with LM Studio, Ollama, Groq, any OpenAI-compatible endpoint |
| `codex_oauth.rs` | `CodexOAuthProvider` — direct OpenAI API via device-code OAuth flow (separate from CLI wrapper) |
| `claude_cli.rs` | Claude Code CLI wrapper (feature: `provider-claude-cli`) |
| `codex_cli.rs` | Codex CLI wrapper (feature: `provider-codex-cli`) |
| `opencode_cli.rs` | OpenCode CLI wrapper (feature: `provider-opencode-cli`) |
| `fallback.rs` | `FallbackProvider` — ordered chain with sticky-fallback logic |
| `retry.rs` | `RetryConfig` — exponential backoff with jitter, Retry-After support |
| `rate_limiter.rs` | Per-model rate limiting (e.g. OpenRouter `:free` tier at 15 req/min) |
| `rig_adapter.rs` | rig-core adapter for building custom provider rigs |
| `json_repair.rs` | LLM JSON output repair/cleanup |
| `streaming_utils.rs` | SSE/streaming response parsing |
| `model_fetch.rs` | Live model list fetching from provider APIs |
| `nonstream_compat.rs` | Non-streaming compatibility shim |
| `bare_tool_call_extractor.rs` | Raw tool call extraction from LLM response text |
| `placeholder.rs` | `PlaceholderProvider` — no-op provider for testing |

## Agent Service (`src/brain/agent/`)

### `service/`

| File | Role |
|---|---|
| `mod.rs` | Module structure, re-exports `AgentService`, public detection functions |
| `builder.rs` | `AgentService` struct — holds provider, tool registry, session map, service context (~835 lines) |
| `tool_loop.rs` | Main agent tool execution loop (~4886 lines) — LLM call → parse response → execute tools → continue/return |
| `context.rs` | Context window management (`service/context.rs`) |
| `compaction.rs` | Two-tier context compaction: 65% soft (async LLM summary), 90% hard (emergency truncation) |
| `compaction_prompts.rs` | Compaction prompt templates |
| `gaslighting.rs` | Gaslighting preamble detection — catches false "tools unavailable" claims alongside valid tool_use |
| `phantom.rs` | Phantom tool-call detection — intent phrases + file-path corroboration |
| `phantom_lang/` | Multi-language phantom detection data: `en.toml`, `es.toml`, `fr.toml`, `pt.toml`, `ru.toml` |
| `feedback.rs` | User feedback injection into the feedback ledger |
| `truncation.rs` | Message truncation utilities |
| `types.rs` | Service-level types (`AgentResponse`, `AgentStreamResponse`, `ProgressEvent`, callbacks) |
| `helpers.rs` | Shared helpers (`detect_text_repetition`) |
| `messaging.rs` | Message handling utilities |

### Other agent files

| File | Role |
|---|---|
| `context.rs` | `AgentContext` — per-session state (messages, system brain, token count, tracked files) |
| `error.rs` | `AgentError` enum |
| `mod.rs` | Module re-exports |

## Tools (`src/brain/tools/`)

### Module system

| File | Role |
|---|---|
| `mod.rs` | Tool module declarations with Cargo feature gates |
| `trait.rs` | `Tool` trait — `name()`, `description()`, `execute()`, `parameters()` |
| `registry.rs` | `ToolRegistry` — tool registration, parameter alias correction |
| `modules.rs` | Tool module grouping/disable system |
| `error.rs` | `ToolError` enum, `ToolResult` type alias |
| `fuzzy.rs` | Fuzzy matching utilities |

### File I/O & Search

| File | Role |
|---|---|
| `read.rs` | Read file contents |
| `write.rs` | Write file contents |
| `edit.rs` | Edit file (string replacement) |
| `ls.rs` | List directory contents |
| `glob.rs` | Glob pattern file search |
| `grep.rs` | Content regex search |
| `hashline/` | Hashline-based precise editing (`edit.rs`, `hash.rs`, `types.rs`) |

### Web Search

| File | Role |
|---|---|
| `web_search.rs` | Web search tool |
| `brave_search.rs` | Brave Search API integration |
| `exa_search.rs` | Exa Search API integration |

### Memory / Session / Channel Search

| File | Role |
|---|---|
| `memory_search.rs` | Long-term memory search |
| `session_search.rs` | Session history search |
| `channel_search.rs` | Channel message search |

### Browser (CDP Chrome Automation)

| File | Role |
|---|---|
| `browser/mod.rs` | Module declarations |
| `browser/manager.rs` | Browser instance manager |
| `browser/navigate.rs` | Navigate to URL |
| `browser/click.rs` | Click element |
| `browser/type_text.rs` | Type text into element |
| `browser/screenshot.rs` | Take screenshot |
| `browser/eval.rs` | Run JavaScript |
| `browser/find.rs` | Find element on page |
| `browser/wait.rs` | Wait for condition |
| `browser/content.rs` | Get page content |
| `browser/close.rs` | Close browser |

### Subagent (Multi-Agent)

| File | Role |
|---|---|
| `subagent/mod.rs` | Module declarations |
| `subagent/manager.rs` | Subagent session manager |
| `subagent/agent_type.rs` | Agent type definitions |
| `subagent/spawn.rs` | Spawn subagent |
| `subagent/wait.rs` | Wait for subagent |
| `subagent/close.rs` | Close subagent |
| `subagent/resume.rs` | Resume subagent |
| `subagent/send_input.rs` | Send input to subagent |
| `subagent/status.rs` | Subagent status |
| `subagent/team/mod.rs` | Team module declarations |
| `subagent/team/create.rs` | Create agent team |
| `subagent/team/delete.rs` | Delete agent team |
| `subagent/team/broadcast.rs` | Broadcast message to team |
| `subagent/team/manager.rs` | Team lifecycle manager |

### RSI Tools

| File | Role |
|---|---|
| `feedback_record.rs` | Record user feedback |
| `feedback_analyze.rs` | Analyze feedback patterns |
| `self_improve.rs` | Self-improvement tool |
| `rsi_proposals.rs` | List RSI proposals |
| `rsi_propose.rs` | Propose RSI improvement |

### Channel Integrations

| File | Role |
|---|---|
| `telegram_connect.rs` | Connect Telegram channel |
| `telegram_send.rs` | Send Telegram message |
| `discord_connect.rs` | Connect Discord channel |
| `discord_send.rs` | Send Discord message |
| `slack_connect.rs` | Connect Slack channel |
| `slack_send.rs` | Send Slack message |
| `whatsapp_connect.rs` | Connect WhatsApp channel |
| `whatsapp_send.rs` | Send WhatsApp message |
| `trello_connect.rs` | Connect Trello channel |
| `trello_send.rs` | Send Trello message |

### Other Tools

| File | Role |
|---|---|
| `bash.rs` | Shell command execution |
| `code_exec.rs` | Code execution sandbox |
| `doc_parser.rs` | Document parsing (PDF, etc.) |
| `http.rs` | HTTP request tool |
| `generate_image.rs` | Image generation |
| `analyze_image.rs` | Image analysis |
| `analyze_video.rs` | Video analysis |
| `task.rs` | Task management |
| `plan_tool.rs` | Planning tool |
| `cron_manage.rs` | Cron job management |
| `config_tool.rs` | Config read/write |
| `context.rs` | Session context tool |
| `notebook.rs` | Notebook editing |
| `follow_up_question.rs` | Follow-up question |
| `a2a_send.rs` | Agent-to-agent send |
| `load_brain_file.rs` | Load brain file into context |
| `write_stemcell_file.rs` | Write stemcell file |
| `rename_session.rs` | Rename current session |
| `tool_manage.rs` | Tool management meta-tool |
| `rebuild.rs` | Rebuild tool |
| `evolve.rs` | Evolve tool |
| `dynamic/` | Dynamic user-defined tools (`loader.rs`, `tool.rs`) |
| `provider_vision.rs` | Provider-specific vision capabilities |
| `brain_file_safety.rs` | Brain file safety checks |

## Other Brain Files (`src/brain/`)

| File | Role |
|---|---|
| `rsi.rs` | RSI core — background engine: feedback digest, periodic analysis, autonomous improvement |
| `rsi_proposals.rs` | RSI proposal system — TOML inboxes for tool/command proposals, applied/rejected archiving |
| `rsi_subsystem.rs` | RSI bash command subsystem classifier (gh, git, docker, cargo patterns) |
| `rsi_sync.rs` | RSI upstream template sync |
| `rsi_git_history.rs` | RSI git history analysis |
| `rsi_pruned.rs` | Pruned RSI improvements tracking |
| `prompt_builder.rs` | Dynamic system prompt assembly from brain files |
| `tokenizer.rs` | `count_tokens()` / `count_message_tokens()` via tiktoken `cl100k_base` |
| `commands.rs` | `CommandLoader`, `UserCommand` — slash commands from `commands.toml` |
| `skills.rs` | SKILL.md workflow system — built-in embedded skills + `~/.stemcell/skills/` user overlay |
| `filter.rs` | Brain file content filter — strips empty sections at read time |
| `dedup_scan.rs` | Brain file deduplication scanner |
| `self_update.rs` | `SelfUpdater` — build, test, hot-restart |
| `plans.rs` | Bundled reference plan files for plan tool |
| `mod.rs` | Module declarations and re-exports |

## Mission Control (`src/brain/mission_control/`)

| File | Role |
|---|---|
| `types.rs` | Shared types (`McInboxItem`, `McActivity`, `McScheduleItem`, `McInboxDetail`) |
| `inbox_service.rs` | RSI proposal inbox — reads pending tool/command/skill proposals |
| `activity_service.rs` | Activity feed — parses `improvements.md` journal |
| `schedule_service.rs` | Cron schedule queue |
| `mod.rs` | Module declarations |
