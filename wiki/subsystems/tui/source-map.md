# TUI Source Map

## Core

| File | Purpose |
|---|---|
| `src/tui/runner.rs` | TUI event loop — main entry point (`run()`) |
| `src/tui/mod.rs` | Re-exports, public API, `run()` convenience fn |
| `src/tui/events.rs` | Event system — keyboard, mouse, resize event polling |
| `src/tui/error.rs` | Error display / rendering for TUI errors |
| `src/tui/highlight.rs` | Syntax highlighting via syntect |
| `src/tui/markdown.rs` | Markdown → TUI rendering via pulldown-cmark |
| `src/tui/plan.rs` | Plan display types and data structures |
| `src/tui/plan_tests.rs` | Tests for plan rendering |
| `src/tui/prompt_analyzer.rs` | Prompt quality/structure analysis |
| `src/tui/provider_selector.rs` | Provider/model interactive selector |
| `src/tui/onboarding_render.rs` | Onboarding wizard rendering logic |

## App Layer (`src/tui/app/`)

| File | Purpose |
|---|---|
| `mod.rs` | `App` struct, lifecycle, main update loop |
| `state.rs` | TUI app state machine |
| `input.rs` | Input handling and buffering |
| `messaging.rs` | TUI messaging/event dispatch |
| `dialogs.rs` | Dialog system (confirmation, prompts, etc.) |
| `background_session.rs` | Background parallel session management |
| `mission_control/mod.rs` | Mission Control sub-app — module root |
| `mission_control/state.rs` | Mission Control app state |
| `mission_control/input.rs` | Mission Control input handling |
| `mission_control/actions.rs` | Mission Control action dispatch |
| `skills_dialog/mod.rs` | Skills picker sub-app — module root |
| `skills_dialog/state.rs` | Skills dialog state |
| `skills_dialog/input.rs` | Skills dialog input handling |
| `skills_dialog/actions.rs` | Skills dialog action dispatch |
| `statusline_dialog/mod.rs` | Statusline dialog — module root |
| `statusline_dialog/state.rs` | Statusline dialog state |
| `statusline_dialog/input.rs` | Statusline dialog input handling |
| `statusline_dialog/actions.rs` | Statusline dialog action dispatch |

## Rendering (`src/tui/render/`)

| File | Purpose |
|---|---|
| `mod.rs` | Main render dispatch — routes to panel renderers |
| `chat.rs` | Chat panel rendering |
| `input.rs` | Input area rendering |
| `help.rs` | Help overlay rendering |
| `panes.rs` | Split pane rendering |
| `sessions.rs` | Session list sidebar rendering |
| `tools.rs` | Tool call rendering |
| `plan_widget.rs` | Plan display widget |
| `plan_window.rs` | Plan detail window/popup |
| `palette.rs` | Color palette and theme definitions |
| `dialogs.rs` | Dialog/widget rendering |
| `utils.rs` | Shared render utilities |
| `mission_control/mod.rs` | Mission Control render module |
| `mission_control/layout.rs` | Mission Control panel layout |
| `mission_control/dispatch.rs` | Mission Control render dispatch |
| `mission_control/activity_panel.rs` | Activity feed panel |
| `mission_control/inbox_panel.rs` | RSI inbox panel |
| `mission_control/schedule_panel.rs` | Cron schedule panel |
| `mission_control/detail_popup.rs` | Detail popup rendering |
| `mission_control/theme.rs` | Mission Control theme |
| `skills_dialog/mod.rs` | Skills dialog render module |
| `skills_dialog/card.rs` | Skills card rendering |
| `skills_dialog/dispatch.rs` | Skills dialog render dispatch |
| `statusline_dialog/mod.rs` | Statusline dialog render module |
| `statusline_dialog/dispatch.rs` | Statusline dialog render dispatch |

## Components (`src/tui/components/`)

| File | Purpose |
|---|---|
| `mod.rs` | Shared component exports |
| `logo.rs` | ASCII logo rendering |

## Pane System (`src/tui/pane/`)

| File | Purpose |
|---|---|
| `mod.rs` | Pane types and exports |
| `layout.rs` | Pane layout calculation (horizontal/vertical splits) |
| `state.rs` | Pane state management |

## Onboarding (`src/tui/onboarding/`)

| File | Purpose |
|---|---|
| `mod.rs` | Onboarding module exports |
| `wizard.rs` | Wizard orchestrator |
| `config.rs` | Config setup step |
| `keys.rs` | API keys entry step |
| `brain.rs` | Brain selection step |
| `channels.rs` | Channel configuration step |
| `voice.rs` | Voice/TTS setup step |
| `models.rs` | Model selection step |
| `fetch.rs` | Data fetching step |
| `types.rs` | Wizard types and step enum |
| `input.rs` | Wizard input handling |
| `navigation.rs` | Step navigation logic |
| `helpers.rs` | Shared wizard utilities |
