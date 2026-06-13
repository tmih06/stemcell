# Brain Tests

## Test Count

- **~3063** `#[test]` annotations across `src/` (measured via `grep -c '#\[test\]' src/ --include='*.rs'`)
- **228 test files** in `src/tests/` (each tests a specific module or feature)
- Inline `#[cfg(test)] mod tests` blocks in most brain source files

## Test Organization

| Location | Description |
|---|---|
| `src/tests/` | Integration & feature-gated tests (228 files) |
| `src/brain/**/*.rs` | Inline `#[cfg(test)]` tests within each module |
| `src/brain/prompt_builder_tests.rs` | Prompt builder inline tests |
| `src/brain/tools/load_brain_file_tests.rs` | Load brain file inline tests |
| `src/brain/tools/write_stemcell_file_tests.rs` | Write stemcell file inline tests |
| `src/brain/tools/test_data/` | Test fixture data for tools |
| `src/benches/database.rs` | Database benchmarks |
| `src/benches/memory.rs` | Memory benchmarks |

## Complete Test File Index (`src/tests/`)

All 227 test files in `src/tests/`, grouped by area:

| Area | Test Files |
|------|-----------|
| Agent | `agent_approval_policies_test.rs`, `agent_basic_test.rs`, `agent_context_tracking_test.rs`, `agent_model_selection_test.rs`, `agent_parallel_sessions_test.rs`, `agent_service_mocks.rs`, `agent_streaming_usage_test.rs`, `agent_tool_normalization_test.rs` |
| Auto-title | `auto_title_e2e_test.rs`, `auto_title_test.rs` |
| Background session | `background_session_test.rs` |
| Bash | `bash_feedback_enrichment_test.rs`, `bash_interactive_reject_test.rs`, `bash_posix_quote_test.rs`, `bash_retry_loop_test.rs`, `bash_ssh_detection_test.rs` |
| Baseline merge | `baseline_merge_test.rs` |
| Brain files | `brain_file_generic_guard_test.rs`, `brain_file_safety_test.rs`, `brain_filter_strip_empty_sections_test.rs`, `brain_templates_test.rs` |
| Browser | `browser_close_test.rs`, `browser_default_linux_test.rs`, `browser_default_test.rs`, `browser_default_windows_test.rs`, `browser_drop_test.rs`, `browser_e2e_test.rs`, `browser_eval_cap_test.rs`, `browser_find_test.rs`, `browser_health_test.rs`, `browser_locks_test.rs`, `browser_profile_wait_test.rs`, `browser_screenshot_surface_test.rs`, `browser_session_test.rs`, `browser_stealth_test.rs` |
| Bundled plans | `bundled_plans_test.rs` |
| Candle/Whisper | `candle_whisper_test.rs` |
| Channels | `channel_commands_test.rs`, `channel_search_test.rs`, `channel_session_resolve_test.rs` |
| CLI | `cli_arg_too_long_test.rs`, `cli_supported_models_test.rs`, `cli_test.rs` |
| CLI providers | `claude_cli_model_test.rs`, `codex_cli_test.rs`, `opencode_provider_test.rs` |
| Compaction | `compaction_prompts_test.rs`, `compaction_test.rs` |
| Config | `config_watcher_test.rs`, `custom_provider_cache_autoenable_test.rs`, `custom_provider_no_models_test.rs`, `custom_provider_rename_keys_toml_test.rs`, `custom_provider_section_resolver_test.rs`, `custom_provider_test.rs`, `provider_config_regression_test.rs`, `provider_factory_regression_test.rs` |
| Context window | `context_window_test.rs` |
| Cron | `cron_test.rs` |
| Cross-provider | `cross_provider_model_leak_guard_test.rs` |
| Custom model | `custom_model_paste_test.rs` |
| Daemon health | `daemon_health_test.rs` |
| Discord | `discord_handler_test.rs` |
| Doc parser | `doc_parser_page_range_test.rs` |
| Dynamic tools | `dynamic_tool_coerce_test.rs` |
| Error scenarios | `error_scenarios_test.rs` |
| Evolve | `evolve_diagnose_test.rs`, `evolve_systemd_restart_test.rs`, `evolve_test.rs`, `post_evolve_test.rs` |
| Exa search | `exa_search_test.rs` |
| Fallback | `fallback_streak_test.rs`, `fallback_vision_test.rs` |
| File extract | `file_extract_test.rs` |
| Follow-up question | `follow_up_intermediate_flush_test.rs`, `follow_up_question_test.rs` |
| Format user error | `format_user_error_test.rs` |
| Gemini | `gemini_fetch_test.rs`, `gemini_schema_sanitize_test.rs` |
| Generate image | `generate_image_backend_test.rs` |
| Git branch | `git_branch_test.rs` |
| GitHub provider | `github_provider_test.rs` |
| Hashline | `hashline_test.rs` |
| HTML/strip | `html_comment_strip_test.rs`, `orphan_close_tag_strip_test.rs` |
| HTTP request | `http_request_test.rs` |
| Image util | `image_util_test.rs` |
| Integration | `integration_test.rs` |
| Input/UI | `altgr_input_test.rs`, `ctrl_o_toggle_test.rs` |
| Kimi reasoning | `kimi_reasoning_test.rs` |
| Local provider | `local_provider_gate_test.rs` |
| Merge keys | `merge_provider_keys_test.rs` |
| Mission Control | `mission_control_activity_service_test.rs`, `mission_control_dedup_detail_test.rs`, `mission_control_inbox_service_test.rs`, `mission_control_input_test.rs`, `mission_control_layout_test.rs`, `mission_control_schedule_service_test.rs`, `mission_control_skill_inbox_test.rs` |
| Model | `model_capability_filter_test.rs`, `model_fetch_test.rs`, `model_selector_refresh_test.rs` |
| Mouse/keyboard | `mouse_fragment_filter_test.rs` |
| New session pane | `new_session_pane_binding_test.rs` |
| Nonstream compat | `nonstream_compat_test.rs` |
| Onboarding | `onboarding_brain_test.rs`, `onboarding_custom_model_input_test.rs`, `onboarding_field_nav_test.rs`, `onboarding_keys_test.rs`, `onboarding_model_refresh_test.rs`, `onboarding_navigation_test.rs`, `onboarding_no_silent_commit_test.rs`, `onboarding_types_test.rs`, `onboarding_user_scroll_test.rs`, `onboarding_welcome_test.rs`, `onboarding_wizard_test.rs` |
| OpenAI provider | `openai_provider_test.rs` |
| PDF | `pdf_page_range_parser_test.rs`, `pdf_smart_routing_test.rs`, `pdf_vision_test.rs` |
| Phantom | `analysis_intent_nudge_test.rs`, `phantom_cleanup_intent_test.rs`, `phantom_db_persistence_test.rs`, `phantom_deferment_test.rs`, `phantom_post_success_exemption_test.rs`, `phantom_pronoun_drop_test.rs` |
| Plans | `plan_document_test.rs`, `plan_mode_integration_test.rs`, `plan_tool_description_test.rs`, `plan_tool_test.rs`, `plan_window_test.rs` |
| Profile | `profile_test.rs` |
| Prompt | `prompt_compiled_features_test.rs`, `prompt_disabled_tool_leak_test.rs`, `prompt_inline_edit_directive_test.rs`, `prompt_known_paths_test.rs` |
| Provider | `provider_error_proxy_test.rs`, `provider_picker_setup_hint_test.rs`, `provider_registry_test.rs`, `provider_sync_test.rs` |
| QR render | `qr_render_test.rs` |
| Qwen | `qwen_detect_test.rs`, `qwen_tool_extractor_test.rs`, `qwen_tool_marker_strip_test.rs` |
| Rate limiter | `rate_limiter_test.rs` |
| Recent paths | `recent_paths_test.rs` |
| Rename session | `rename_session_test.rs` |
| RSI | `rsi_brain_dedup_test.rs`, `rsi_fallback_wrap_test.rs`, `rsi_git_history_test.rs`, `rsi_proposals_test.rs`, `rsi_pruned_test.rs`, `rsi_skill_proposals_test.rs`, `rsi_subsystem_test.rs`, `rsi_sync_cap_bail_test.rs`, `rsi_sync_test.rs`, `rsi_test.rs` |
| RTK | `rtk_rewrite_test.rs`, `rtk_sysadmin_supported_test.rs`, `rtk_tracker_test.rs` |
| Runtime info | `runtime_info_home_anchor_test.rs` |
| Sanitize | `sanitize_code_edit_block_test.rs`, `sanitize_redaction_test.rs` |
| Self-healing | `self_healing_test.rs` |
| Self-improve | `self_improve_failure_log_guard_test.rs` |
| Session | `session_chat_id_lookup_test.rs`, `session_provider_wrap_test.rs`, `session_working_dir_test.rs` |
| Skills | `skill_slash_dispatch_test.rs`, `skills_dialog_test.rs`, `skills_test.rs` |
| Slack | `slack_fmt_test.rs`, `slack_handler_test.rs` |
| Split pane | `split_pane_test.rs` |
| Stream loop | `stream_loop_test.rs` |
| Streaming | `streaming_active_secs_test.rs`, `streaming_test.rs`, `streaming_tps_accumulator_test.rs` |
| STT/TTS | `stt_fallback_chain_test.rs`, `tts_fallback_chain_test.rs`, `voice_local_tts_test.rs`, `voice_local_whisper_test.rs`, `voice_onboarding_test.rs`, `voice_openai_compatible_test.rs`, `voice_service_test.rs`, `voice_stt_dispatch_test.rs`, `voice_voicebox_test.rs` |
| Subagent | `subagent_test.rs`, `subagent_tool_description_test.rs`, `wait_agent_resolver_test.rs` |
| Telegram | `telegram_handler_test.rs`, `telegram_join_detection_test.rs`, `telegram_photo_batching_test.rs`, `telegram_plan_render_test.rs`, `telegram_pre_tool_rolling_test.rs`, `telegram_quote_reply_test.rs`, `telegram_resume_test.rs`, `telegram_send_thread_id_override_test.rs`, `telegram_session_resolve_test.rs`, `telegram_status_message_test.rs`, `telegram_thread_id_lookup_test.rs`, `telegram_topic_listing_test.rs` |
| Text complete | `text_complete_test.rs` |
| Token tracking | `token_tracking_test.rs` |
| Tool execution | `tool_execution_repo_test.rs`, `tool_loop_helpers_test.rs` |
| Tool args/regression | `tool_arg_unescape_test.rs`, `tools_md_regression_test.rs` |
| TUI | `tui_error_test.rs`, `tui_render_clear_test.rs`, `tui_tool_stack_test.rs` |
| Usage | `usage_activity_columns_test.rs`, `usage_cosmetic_alias_test.rs`, `usage_grouping_test.rs`, `usage_ledger_test.rs` |
| User correction | `user_correction_metadata_test.rs` |
| Web/browser routing | `web_browser_routing_test.rs` |
| WhatsApp | `whatsapp_handler_test.rs`, `whatsapp_photo_batching_test.rs`, `whatsapp_state_test.rs` |
| Misc | `collapse_build_output_test.rs`, `collapse_home_test.rs`, `handshake_timeout_test.rs`, `intermediate_text_strip_guard_test.rs`, `queued_message_test.rs`, `reasoning_lines_test.rs`, `slash_autocomplete_dimensions_test.rs`, `system_continuation_test.rs` |

## Feature Gating

Tests compile **only when their corresponding Cargo feature is enabled**. This means:

```sh
# Run all tests (all features)
cargo test --all-features

# Run a specific test (requires its feature)
cargo test --all-features <test_name>

# Run brain-related tests only
cargo test --all-features brain
```

Examples of feature-gated test files (from `src/tests/`):

| Test file | Requires feature |
|---|---|
| `claude_cli_model_test.rs` | `provider-claude-cli` |
| `codex_cli_test.rs` | `provider-codex-cli` |
| `opencode_provider_test.rs` | `provider-opencode-cli` |
| `browser_e2e_test.rs` | `tool-browser-*` |
| `telegram_handler_test.rs` | `tool-telegram-*` |
| `discord_handler_test.rs` | `tool-discord-*` |
| `slack_handler_test.rs` | `tool-slack-*` |
| `whatsapp_handler_test.rs` | `tool-whatsapp-*` |
| `cron_test.rs` | `tool-cron-manage` |
| `subagent_test.rs` | `tool-spawn-agent` |
| `plan_tool_test.rs` | `tool-plan` |

## Snapshot Testing

Insta snapshot testing (`insta = "1.42"` with `json` + `yaml` features) is used across many test files:

- `src/tests/fallback_streak_test.rs`
- `src/tests/auto_title_e2e_test.rs`
- `src/tests/agent_parallel_sessions_test.rs`
- `src/tests/browser_health_test.rs`
- `src/tests/browser_default_test.rs`
- `src/tests/browser_profile_wait_test.rs`
- `src/tests/rtk_rewrite_test.rs`
- `src/tests/profile_test.rs`
- `src/tests/streaming_tps_accumulator_test.rs`
- `src/tests/browser_stealth_test.rs`
- `src/tests/cli_supported_models_test.rs`
- `src/tests/rate_limiter_test.rs`
- `src/tests/rsi_proposals_test.rs`
- `src/tests/self_healing_test.rs`
- `src/tests/evolve_test.rs`
- `src/tests/baseline_merge_test.rs`
- `src/tests/follow_up_question_test.rs`
- `src/tests/evolve_systemd_restart_test.rs`
- ...and more

## Key Test Areas

Brain tests cover:
- **Agent**: basic conversations, tool normalization, parallel sessions, streaming, model selection, approval policies
- **Providers**: Anthropic, Gemini, Qwen, OpenAI-compat, CLI wrappers, fallback chain, rate limiting, model fetch
- **Tools**: bash, file I/O, browser automation, subagents, cron, web search, HTTP, image/video, document parsing
- **RSI**: proposals, dedup, git history, subsystem analysis, sync, pruned improvements
- **Compaction**: soft/hard thresholds, prompt templates, integration tests
- **Phantom**: intent detection, pronoun-drop, cleanup intent, deferment, post-success exemption
- **Gaslighting**: detection patterns, strip logic
- **Tokenizer**: token counting accuracy
- **Mission Control**: inbox, activity, schedule services

## Test Commands

```sh
# Full test suite
cargo test --all-features

# Minimal (default features only)
cargo test

# Single test
cargo test --all-fefficiency compaction_test

# Brain module tests
cargo test --all-features -- brain

# With snapshot review
cargo insta review
```

## Benchmarks

| File | Description |
|---|---|
| `src/benches/database.rs` | Database query benchmarks |
| `src/benches/memory.rs` | Memory subsystem benchmarks |

```sh
cargo bench --all-features
```
