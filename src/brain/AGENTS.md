# src/brain/ — Agent Core

The AI core: LLM providers, the agent tool loop, ~40 tools, tokenizer, prompt
assembly, RSI, and mission control. Read this before touching `src/brain/`.
Use `codegraph query <symbol>` to locate things; `codegraph impact <symbol>`
before changing anything in the high-risk files below.

> The Tool trait in `wiki/contracts.md` is STALE. The source in
> `src/brain/tools/trait.rs` is truth — see section 2.

## Structure

| Dir | Owns |
|-----|------|
| `provider/` | 15+ LLM backends. `trait.rs` (Provider trait), `types.rs` (LLMRequest/Response, Message, ContentBlock, StreamEvent), `factory.rs` (config-driven creation), `fallback.rs`, `retry.rs`, `rate_limiter.rs`. Native: `anthropic.rs`, `gemini.rs`, `copilot.rs`, `qwen.rs`, `custom_openai_compatible.rs`. CLI wrappers (feature-gated): `claude_cli.rs`, `codex_cli.rs`, `opencode_cli.rs`. |
| `agent/` | Per-session state: `agent/context.rs` (`AgentContext`: messages, system brain, token_count, tracked files), `agent/error.rs`. |
| `agent/service/` | Orchestration. `builder.rs` (`AgentService`), `tool_loop.rs` (main loop, ~4900 lines), `context.rs`, `compaction.rs` + `compaction_prompts.rs`, `gaslighting.rs`, `phantom.rs` + `phantom_lang/`, `helpers.rs`, `truncation.rs`, `messaging.rs`, `feedback.rs`, `types.rs`. |
| `tools/` | `trait.rs` (Tool trait), `registry.rs` (storage/dispatch/param-aliases), `modules.rs` (registration — source of truth), one file per tool. Subdirs: `hashline/`, `browser/`, `subagent/` (+`team/`), `dynamic/`. |
| `mission_control/` | RSI panels backing the TUI: `inbox_service.rs`, `activity_service.rs`, `schedule_service.rs`, `types.rs`. |
| `brain/` (top) | `tokenizer.rs` (tiktoken cl100k_base), `prompt_builder.rs`, `rsi*.rs`, `commands.rs`, `skills.rs`, `self_update.rs`, `filter.rs`, `plans.rs`. |

## Adding a Tool

The real trait (`src/brain/tools/trait.rs`):

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;                 // NOT String
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;        // NOT parameters()
    fn capabilities(&self) -> Vec<ToolCapability>;
    fn requires_approval(&self) -> bool { /* default: true if Write/Shell/SystemMod */ }
    fn requires_approval_for_input(&self, input: &Value) -> bool { self.requires_approval() }
    async fn execute(&self, input: Value, context: &ToolExecutionContext) -> Result<ToolResult>; // NOT run()
    fn validate_input(&self, _input: &Value) -> Result<()> { Ok(()) }
}
```

`ToolExecutionContext` carries `session_id`, `working_directory` (use
`.working_dir()` to honor runtime `/cd`), `env_vars`, `auto_approve`,
`timeout_secs`, `service_context`, and callbacks (`sudo_callback`, `ssh_callback`,
`question_callback`). Build results with `ToolResult::success(String)` /
`::error(String)` / `.with_metadata()` / `.with_images()`.

Steps:

1. Create `src/brain/tools/<name>.rs` with a unit struct implementing `Tool`.
   Model it on `src/brain/tools/read.rs` (`ReadTool`, name `"read_file"`).
2. Add `tool-<name> = []` in `Cargo.toml` `[features]` and add it to the right
   umbrella feature (`tools-file-ops`, `tools-search`, `tools-meta`, …).
3. Declare the module in `src/brain/tools/mod.rs` behind `#[cfg(feature = "tool-<name>")]`.
4. Register in `src/brain/tools/modules.rs`: find the relevant `ToolModule`
   (`FileOpsModule`, `WorkflowModule`, `MetaModule`, …), add inside its
   `register()`: `#[cfg(feature="tool-<name>")] ctx.register(Arc::new(super::<name>::FooTool));`.
   Add your feature to that module's `#[cfg(any(...))]` gate AND to the matching
   gate in `all_modules()`. New module → impl `ToolModule` and push it in `all_modules()`.
5. Test in `src/tests/<name>_test.rs`, register the mod in `src/tests/mod.rs`.

**You do NOT edit `registry.rs` to add a tool** — registration is via
`modules.rs`. Missing registration = silent omission from the agent's tool list.
Entry point is `register_enabled_tools(_with_runtime)` in `modules.rs`, called
from `src/cli/ui.rs` (Full mode) and `src/cli/commands.rs` (Minimal mode).

## Adding a Provider

Provider trait (`src/brain/provider/trait.rs`, wiki contract here IS accurate):
`complete()`, `stream()`, `supports_tools/streaming/vision()`, `name()`,
`default_model()`, `supported_models()`, `context_window()`, `calculate_cost()`,
plus fallback hooks (`force_next_fallback`, `take_swap_event`, `is_fallback_chain`,
`active_subprovider_name/model`).

1. Create `src/brain/provider/<name>.rs` (model on `anthropic.rs` or
   `custom_openai_compatible.rs`).
2. Add `try_create_<name>(config) -> Result<Option<Arc<dyn Provider>>>` in `factory.rs`.
3. Register in the `REGISTRATIONS: LazyLock<Vec<ProviderRegistration>>` array
   (`factory.rs`): `{ display_name, session_id, aliases, is_enabled,
   factory: sync_factory(try_create_<name>), config_field }`. Use `sync_factory(...)`
   for sync fns, or `Box::new(|c| Box::pin(try_create_<name>(c)))` for async.
4. Add display_name to the `PROVIDER_NAMES` const (same priority order).
5. Add `providers.<name>` in `src/config/types.rs`, example keys in `keys.toml.example`.
6. Test (see `provider_factory_regression_test.rs`, `provider_registry_test.rs`).

Reference: `wiki/reference/ADDING_NEW_PROVIDERS.md`.

## Agent Loop (high level)

User message → `AgentService` (builder.rs) resolves per-session `Arc<dyn Provider>`
and builds `AgentContext` → `run_tool_loop` (tool_loop.rs). Each iteration: build
`LLMRequest` → `provider.complete()`/`.stream()` → parse (`bare_tool_call_extractor.rs`,
`json_repair.rs`) → phantom-intent check (`phantom.rs`) + gaslighting-strip
(`gaslighting.rs`) → execute tools via `ToolRegistry::execute()` → append results
→ check context budget (`compaction.rs`) → loop if tool calls were made, else
return. A notification fires after every `run_tool_loop` completion.

## Gotchas

- **Feature gates live in 3 places** that must stay in sync: the module `#[cfg]`,
  the `all_modules()` `#[cfg]`, and `Cargo.toml` umbrella features. `build.rs` +
  `src/scripts/tool_features.py` + `build_toggles.toml` cross-check them.
- **Two-level disabling**: compile-time (Cargo features, drops code) vs runtime
  (`config.toml [tools] disabled = [...]`). `disabled = ["all"]` = chatbot mode.
- **phantom_lang/**: detects "narrated but didn't call a tool" responses using
  multilang TOML (`en/es/fr/pt/ru.toml`). Gates: `has_phantom_tool_intent`
  (strict) and `..._no_tools` (relaxed).
- **Compaction**: 65% soft (async LLM summary, non-blocking), 90% hard (emergency
  truncation, never fails). Uses provider `context_window` from config.
- **hashline editing** (`tools/hashline/`): precise line-targeted edits via 2-char
  content hashes; `read_file` with `hashline:true` emits `HASH|content`, the edit
  tool matches on those hashes.
- **Dynamic tools** (`tools/dynamic/`): user-defined HTTP/shell tools loaded from
  `tools.toml` at runtime via `DynamicToolLoader` (feature `tools-dynamic`).
- **Param aliases**: `registry.rs PARAM_ALIASES` auto-corrects common LLM param
  mistakes (`query`→`pattern`, `old_string`→`old_text`) before validation.
- **factory.rs fallback wrapping** is fragile — double-wrap / naked-provider
  hazards via `is_fallback_chain` / `active_subprovider_name`. Tread carefully.

## Tests

**New tests go in `src/tests/<area>_test.rs`**, registered in `src/tests/mod.rs`
(project policy — see `src/tests/AGENTS.md`). The small inline `#[cfg(test)]` blocks
in `trait.rs`/`registry.rs` are existing unit tests — don't extend that pattern for
new feature tests. Key files: `agent_basic_test.rs`, `agent_*_test.rs`,
`agent_service_mocks.rs` (shared mock provider/harness — use it instead of live
APIs), `compaction_test.rs`, `phantom_*_test.rs`, `hashline_test.rs`,
`dynamic_tool_coerce_test.rs`, `provider_factory_regression_test.rs`,
`provider_registry_test.rs`, `fallback_*_test.rs`, `custom_provider_*_test.rs`,
`<provider>_provider_test.rs`, `qwen_tool_extractor_test.rs`. Find coverage for a
file with `codegraph affected <file>`.
