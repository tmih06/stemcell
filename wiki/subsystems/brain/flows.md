# Brain Flows

## Provider Selection

```
config.toml
  │
  ▼
provider/factory.rs :: create_provider()
  │
  ├─ Iterates REGISTRATIONS array (ProviderRegistration entries)
  ├─ Checks config for matching provider config
  ├─ Calls try_create_* factory function
  │
  ▼
Arc<dyn Provider>
  │
  ├─ If fallback configured → FallbackProvider wraps primary + fallback chain
  │    └─ On rate-limit/retryable error: tries next provider in chain
  │    └─ On success via fallback: becomes sticky (session persists choice)
  │
  ▼
AgentService holds provider as RwLock<Arc<dyn Provider>>
```

**Key files:**
- `src/brain/provider/factory.rs` — registry pattern with 18+ registered providers
- `src/brain/provider/fallback.rs` — `SwapEvent` on fallback activation
- `src/brain/provider/mod.rs` — `create_provider_by_name()`, `create_provider_with_warning()` re-exports

## Agent Tool Loop

```
User message
  │
  ▼
AgentService (agent/service/builder.rs)
  │
  ├─ provider_for_session(id) → get/create per-session Arc<dyn Provider>
  ├─ agent/service/context.rs  → build agent context (messages + system brain)
  ├─ tokenizer::count_tokens() → estimate token usage
  │
  ▼
tool_loop (agent/service/tool_loop.rs)
  │
  ├─ BUILD LLMRequest from context
  ├─ CALL provider.complete(request) or provider.stream(request)
  ├─ PARSE response
  │    ├─ bare_tool_call_extractor.rs  → extract raw tool calls from text
  │    └─ json_repair.rs               → fix malformed JSON
  │
  ├─ CHECK: phantom detection (phantom.rs)
  │    └─ If phantom → retry with correction ("Please actually execute the tool")
  │
  ├─ CHECK: gaslighting detection (gaslighting.rs)
  │    └─ Strip gaslighting preamble from response
  │
  ├─ EXECUTE each tool call
  │    ├─ ToolRegistry::execute(name, args)
  │    ├─ ToolError → fold into next LLM call
  │    └─ Result → append to messages
  │
  ├─ CHECK: context budget (compaction.rs)
  │    ├─ < 65% → continue
  │    ├─ 65-90% → Tier 1: spawn async LLM compaction (return immediately)
  │    └─ ≥ 90% → Tier 2: emergency truncation (cancel in-flight compaction)
  │
  ├─ CHECK: continuation
  │    ├─ Tool calls found → loop (next LLM call with tool results)
  │    └─ No tool calls → return response to user
  │
  ▼
User sees response
```

**Key files:**
- `src/brain/agent/service/tool_loop.rs` (~4886 lines) — main loop
- `src/brain/agent/service/builder.rs` — `AgentService` struct
- `src/brain/agent/service/context.rs` — context management
- `src/brain/agent/service/messaging.rs` — message handling

## Context Compaction

```
Context grows with each turn
  │
  ▼
Check: token_count / max_tokens
  │
  ├── < 65% threshold ───────────────► No action needed
  │
  ├── ≥ 65% (soft threshold) ─────────► Tier 1: Async LLM Compaction
  │    │
  │    ├─ Spawn background task
  │    ├─ LLM summarizes older messages (using compaction_prompts.rs templates)
  │    ├─ Agent keeps processing turns (non-blocking)
  │    └─ On next visit: check if summary ready → atomic swap
  │
  └── ≥ 90% (hard threshold) ────────► Tier 2: Emergency Truncation
       │
       ├─ Cancel any in-flight async compaction
       ├─ Truncate older messages back to 80% (guaranteed non-blocking)
       └─ NEVER fails
```

**Key files:**
- `src/brain/agent/service/compaction.rs` — two-tier enforcement
- `src/brain/agent/service/compaction_prompts.rs` — LLM summarization prompt templates
- `src/brain/tokenizer.rs` — `count_tokens()` for budget calculation

## Phantom Detection

```
LLM response received
  │
  ▼
has_phantom_tool_intent(response) → phantom.rs
  │
  ├─ Intent phrase check (e.g., "Let me check...", "I'll update...", "Pushed.")
  ├─ Multi-language data from phantom_lang/*.toml (en, es, fr, pt, ru)
  ├─ File-path corroboration for strict path
  │
  ├── Intent detected ───────────────► Retry with correction
  │    │                                ("Please actually execute the tool call")
  │    ▼
  │   Tool loop re-invokes LLM
  │
  └── No intent detected ────────────► Proceed normally
```

**Variants:**
- `has_phantom_tool_intent_no_tools` — relaxed gate used when iteration produced zero tool uses
- `has_phantom_tool_intent` — strict gate for general path (needs multi-step plans, completion claims, or intent + file-path)

**Key files:**
- `src/brain/agent/service/phantom.rs` — detection logic
- `src/brain/agent/service/phantom_lang/` — language-specific TOML data

## Gaslighting Detection

```
Provider response received
  │
  ▼
is_gaslighting_preamble(text) → gaslighting.rs
  │
  ├─ Match against GASLIGHTING_REFUSAL_PHRASES (case-insensitive)
  │    e.g. "tools aren't responding", "tools appear to be unavailable"
  │
  ├── Gaslighting detected ──────────► strip_gaslighting_preamble()
  │    │                                Remove the gaslighting lines
  │    ▼
  │   Remaining text (with valid tool_use blocks) processed normally
  │
  └── No gaslighting ────────────────► Use response as-is
```

**Key files:**
- `src/brain/agent/service/gaslighting.rs` — detection + stripping

## Tool Registration

```
Cargo feature flag (e.g., "tool-bash", "tool-telegram-send")
  │
  ▼
src/brain/tools/mod.rs
  │
  ├─ #[cfg(feature = "tool-bash")] pub mod bash;
  ├─ #[cfg(feature = "tool-browser-navigate")] pub mod browser;
  └─ ... one cfg gate per tool or tool group
  │
  ▼
src/brain/tools/registry.rs
  │
  ├─ ToolRegistry::new() registers all enabled tools
  ├─ Each tool provides name, description, parameters (JSON Schema)
  ├─ Parameter alias correction (e.g., "query" → "pattern", "file" → "path")
  │
  ▼
AgentService uses ToolRegistry for execution dispatch
```

**Key files:**
- `src/brain/tools/mod.rs` — feature-gated module declarations
- `src/brain/tools/registry.rs` — registration + aliases
- `src/brain/tools/trait.rs` — `Tool` trait

## RSI Pipeline

```
User interaction
  │
  ▼
feedback_record.rs / feedback.rs
  │
  ├─ Records success/failure events in feedback ledger (DB)
  ├─ Enriched with bash cmd metadata, tool names, timestamps
  │
  ▼
RSI background engine (rsi.rs)
  │
  ├─ Runs every RSI_CYCLE_INTERVAL_SECS (3600s = 1 hour)
  ├─ Requires RSI_MIN_ENTRIES (50) before first analysis
  ├─ Classifies bash commands by subsystem (rsi_subsystem.rs)
  │
  ▼
feedback_analyze.rs  ───►  Identify patterns / pain points
  │
  ▼
rsi_propose.rs  ───►  Write proposals to TOML inbox
  │                     (~/.stemcell/rsi/proposed_tools.toml)
  │                     (~/.stemcell/rsi/proposed_commands.toml)
  │
  ▼
Mission Control inbox (mission_control/inbox_service.rs)
  │
  ├─ User reviews proposals in TUI
  ├─ Apply → tool_manage / config_manager apply
  └─ Reject → archive to ~/.stemcell/rsi/rejected/
  │
  ▼
self_improve.rs  ───►  Apply accepted improvements to brain files
                        (SOUL.md, AGENTS.md, etc.)
  │
  ▼
Activity feed (mission_control/activity_service.rs)
  ├─ Appends to ~/.stemcell/rsi/improvements.md
  └─ User sees history in TUI activity panel
```

**Key files:**
- `src/brain/rsi.rs` — background engine
- `src/brain/rsi_proposals.rs` — proposal TOML storage
- `src/brain/rsi_subsystem.rs` — command classifier
- `src/brain/tools/feedback_record.rs` — feedback recording
- `src/brain/tools/feedback_analyze.rs` — feedback analysis
- `src/brain/tools/self_improve.rs` — improvement application
- `src/brain/tools/rsi_propose.rs` — proposal creation
- `src/brain/mission_control/` — inbox, activity, schedule

## Tokenizer Usage

```
tokenizer.rs :: count_tokens(text)
  │
  ├─ Uses tiktoken cl100k_base BPE encoding
  ├─ Singleton via Lazy<CoreBPE>
  │
  ├── Used by:
  │    ├─ AgentContext → estimate token count for current conversation
  │    ├─ compaction.rs → calculate context budget (65%/90% thresholds)
  │    ├─ truncation.rs → determine how many messages to trim
  │    ├─ tool_loop.rs → check context before LLM call
  │    └─ prompt_builder.rs → track brain file token cost
  │
  └── count_message_tokens(text) = count_tokens(text) + 4 (role overhead)
```

**Key files:**
- `src/brain/tokenizer.rs` — `count_tokens()`, `count_message_tokens()`
- `src/brain/agent/service/compaction.rs` — threshold enforcement
- `src/brain/agent/context.rs` — `token_count` field
