# Brain Subsystem

**Path:** `src/brain/`

The core AI layer of StemCell — LLM providers, agent orchestration, 30+ tools, tokenizer, prompt assembly, RSI, and self-update.

## Subsystem Map

| Layer | Location | Role |
|---|---|---|
| Providers | `src/brain/provider/` | Abstraction over 10+ LLM backends (Anthropic, Gemini, Copilot, Qwen, OpenAI-compat, CLI wrappers) |
| Agent Service | `src/brain/agent/service/` | Tool loop orchestrator, context management, compaction, gaslighting/phantom detection |
| Tools | `src/brain/tools/` | 30+ tools: bash, file I/O, search, browser, subagents, cron, code exec, channel integrations, RSI, meta-tools |
| Tokenizer | `src/brain/tokenizer.rs` | tiktoken `cl100k_base` BPE token counting |
| Prompt Builder | `src/brain/prompt_builder.rs` | Dynamic system prompt assembly from brain files (SOUL.md, USER.md, AGENTS.md, etc.) |
| RSI | `src/brain/rsi.rs` | Recursive Self-Improvement background engine |
| Mission Control | `src/brain/mission_control/` | RSI inbox, activity feed, cron schedule queue (panels) |
| Skills | `src/brain/skills.rs` | SKILL.md workflow system (built-in + user overlay) |
| Slash Commands | `src/brain/commands.rs` | User-defined `/commands` from `commands.toml` |
| Self-Update | `src/brain/self_update.rs` | Build, test, and hot-restart from source |

## LLM Providers

- **Native APIs:** Anthropic (`anthropic.rs`), Google Gemini (`gemini.rs`), GitHub Copilot (`copilot.rs`), Qwen (`qwen.rs`)
- **OpenAI Compatible:** `custom_openai_compatible.rs` — LM Studio, Ollama, Groq, OpenCode Zen Free, any OpenAI-format endpoint
- **CLI Wrappers:** Claude Code (`claude_cli.rs`), Codex CLI (`codex_cli.rs`), OpenCode CLI (`opencode_cli.rs`)
- **Model Fetching:** Dynamic model fetching supported via `models.dev` cost metadata (e.g., filtering free models for OpenCode Zen Free).
- **Infrastructure:** `fallback.rs` (fallback chain), `retry.rs` (exponential backoff), `rate_limiter.rs` (per-provider pacing), `factory.rs` (config-driven creation)

## Agent Service

Located at `src/brain/agent/service/`, the `AgentService` orchestrates the conversation loop:

- **Tool Loop** (`tool_loop.rs`): Main execution loop — sends user message to LLM, parses tool calls, executes them, loops until complete
- **Context Management** (`context.rs`): Message storage, token tracking, system brain attachment
- **Compaction** (`compaction.rs`): Two-tier — 65% soft threshold triggers async LLM summarization, 90% hard threshold triggers emergency truncation
- **Gaslighting Detection** (`gaslighting.rs`): Catches provider responses that claim tools are unavailable while simultaneously emitting tool_use blocks
- **Phantom Detection** (`phantom.rs`): Detects assistant text that narrates actions without emitting tool calls; multi-language support (`phantom_lang/`: en, es, fr, pt, ru)

## Tools

~30+ tools gated by Cargo feature flags. Categories:

| Category | Tools |
|---|---|
| File I/O | `read`, `write`, `edit`, `ls`, `glob`, `grep`, `hashline` |
| Search | `web_search`, `brave_search`, `exa_search`, `memory_search`, `session_search`, `channel_search` |
| Browser | CDP Chrome automation (`navigate`, `click`, `type`, `screenshot`, `eval`, `find`, `wait`, `content`, `close`) |
| Subagent | Multi-agent orchestration (`spawn`, `wait`, `close`, `resume`, `send_input`, teams) |
| RSI | `feedback_record`, `feedback_analyze`, `self_improve`, `rsi_proposals`, `rsi_propose` |
| Channels | Telegram, Discord, Slack, WhatsApp, Trello (connect/send pairs) |
| Meta | `tool_manage`, `rebuild`, `evolve`, `config_tool`, `slash_command`, `rename_session` |
| Other | `bash`, `code_exec`, `doc_parser`, `http`, `generate_image`, `analyze_image`, `analyze_video`, `task`, `plan_tool`, `cron_manage`, `context`, `notebook`, `follow_up_question`, `a2a_send`, `load_brain_file`, `write_stemcell_file` |

## Key Design Decisions

- **Feature-gated compilation**: Each tool and provider is behind a Cargo feature flag. Building with `--no-default-features` produces a minimal binary.
- **Dynamic brain assembly**: System prompt is rebuilt each turn from brain files, so edits to `SOUL.md`/`AGENTS.md` take effect immediately.
- **Fallback chain**: Providers chain through fallbacks on rate-limit/retryable errors; successful fallback becomes sticky.
- **RSI autonomy with guardrails**: RSI proposes new tools/commands via TOML inbox files; user approval required before installation.
