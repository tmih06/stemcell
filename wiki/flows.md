# Flows

## Startup Flow

```
main.rs → cli::Cli::parse()
       → logging::init_logging()
       → cli::run()
           → match subcommand:
               TUI:     tui::runner::run()
               chat:    interactive REPL
               agent:   AgentService::run()
               daemon:  daemon loop
               service: service mode
               *:       direct command handler
           → Config::load()                    [config/types.rs]
           → Database::init()                  [db/database.rs]
           → ProviderFactory::create()         [brain/provider/factory.rs]
           → ChannelManager::init()            [channels/manager.rs]
           → AgentService::new()               [brain/agent/service/mod.rs]
```

## Request Flow

```
User Input → Channel/TUI
          → AgentService::process_message()   [brain/agent/service/mod.rs]
          → tool_loop::run()                   [brain/agent/service/tool_loop.rs]
              → build messages with context    [context.rs]
              → call Provider::stream()        [brain/provider/trait.rs]
              → parse response for tool calls
              → execute Tool::run()            [brain/tools/trait.rs]
              → collect results
              → repeat until final response
          → Response → Channel/TUI
```

## Provider Call Flow

```
AgentService
  → Provider::stream(request)              [brain/provider/trait.rs]
  → Provider::complete(request)            [non-streaming path]
  → Anthropic/Gemini/Copilot/Qwen/OpenAI-compat
      → HTTP POST to API endpoint
      → SSE streaming (or full response)
      → parse StreamEvent stream
      → return ProviderStream
  → FallbackProvider::stream()             [brain/provider/fallback.rs]
      → try primary, cascade on failure
      → emit SwapEvent on provider change
```

## Tool Execution Flow

```
AgentService::process_message()
  → tool_loop::run()
      → extract tool calls from LLM response
      → lookup Tool in registry            [brain/tools/registry.rs]
      → Tool::run(context, args)           [brain/tools/trait.rs]
      → collect ToolResult
      → add result to message history
      → loop back to LLM
```

## Memory Search Flow

```
memory_search tool
  → hybrid search:
      FTS5 full-text search                 [memory/index.rs]
      Vector similarity search               [memory/embedding.rs]
  → RRF merge (Reciprocal Rank Fusion)
  → return ranked results                    [memory/search.rs]
```

## Compaction Flow

```
context_window approaching limit:
  → soft threshold (65%): warn + scheduled compaction
  → hard threshold (90%): force compaction   [brain/agent/service/compaction.rs]
  → generate continuation document
  → truncate message history
  → save compaction marker to DB
  → subsequent messages use abbreviated history
```

## Channel Message Flow

```
Telegram/Discord/Slack/WhatsApp
  → platform webhook/gateway
  → channel handler                        [channels/<name>/]
  → session_resolve (map to session ID)    [channels/session_resolve.rs]
  → AgentService::process_message()
  → response
  → channel send back                       [channels/<name>/ or brain/tools/<name>_send.rs]
```

## A2A Flow

```
External Agent
  → HTTP POST /rpc                          [a2a/server.rs]
  → JSON-RPC 2.0 dispatch                   [a2a/handler/]
  → AgentService::process_message()
  → SSE stream response
  → A2A task persistence                    [a2a/persistence.rs]
```

## Cron Flow

```
Cron Scheduler loop                         [cron/scheduler.rs]
  → poll DB for due cron jobs               [db/repository/cron_job.rs]
  → execute tool in active session
  → optional channel delivery
  → log run result                          [db/repository/cron_job_run.rs]
```

## RSI Flow

```
Feedback Record                            [brain/tools/feedback_record.rs]
  → store in feedback_ledger                [db/repository/feedback_ledger.rs]
Feedback Analyze                            [brain/tools/feedback_analyze.rs]
  → analyze patterns, produce insights
Proposal Generation                         [brain/rsi_proposals.rs]
  → generate improvement proposals
Self-Improve                                [brain/tools/self_improve.rs]
  → apply proposals under supervision
Mission Control                             [brain/mission_control/]
  → activity feed, inbox, schedule
```

## CI Flow

```
git push → GitHub Actions                  [.github/workflows/]
  → lint (clippy)
  → build (--profile ci --all-features)
  → test (--profile ci --all-features)
  → audit (cargo-audit)
  → coverage (cargo-tarpaulin)
  → deny (cargo-deny)
  → typos
  → secrets (gitleaks)
  → msrv
  → release (on tag)
```
