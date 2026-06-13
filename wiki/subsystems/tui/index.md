# TUI Subsystem

**Location:** [`src/tui/`](../../../src/tui/)

Terminal UI built with [Ratatui](https://ratatui.rs/) v0.30 and [Crossterm](https://crates.io/crates/crossterm) v0.29.

## Features

| Feature | Description | Key Files |
|---|---|---|
| Split panes | Tmux-style horizontal/vertical pane management | `src/tui/pane/` |
| Markdown rendering | Renders markdown via pulldown-cmark | `src/tui/markdown.rs` |
| Syntax highlighting | Code block highlighting via syntect | `src/tui/highlight.rs` |
| Onboarding wizard | Step-by-first-run setup | `src/tui/onboarding/`, `src/tui/onboarding_render.rs` |
| Skills dialog | Browse and select available skills | `src/tui/app/skills_dialog/`, `src/tui/render/skills_dialog/` |
| Export dialog | Export session transcript to clipboard/file | `src/tui/app/export_dialog/`, `src/tui/render/export_dialog/` |
| Mission Control TUI | RSI inbox, activity feed, cron schedule | `src/tui/app/mission_control/`, `src/tui/render/mission_control/` |
| Plan rendering | Display execution plans | `src/tui/plan.rs`, `src/tui/render/plan_widget.rs`, `src/tui/render/plan_window.rs` |
| Background sessions | Parallel session execution | `src/tui/app/background_session.rs` |
| Event handling | Keyboard, mouse, resize events | `src/tui/events.rs` |
| Provider/model selector | LLM provider and model picker | `src/tui/provider_selector.rs` |
| Prompt analyzer | Analyze prompt quality/structure | `src/tui/prompt_analyzer.rs` |

## Architecture

```
events → runner (event loop) → app (update) → render (dispatch) → terminal
```

The event loop in [`runner.rs`](source-map.md#core) polls for input, dispatches to the [`App`](source-map.md#app-layer) state machine, then calls the render pipeline.

## Key Entry Points

- [`runner.rs`](source-map.md#core) — `run()` starts the TUI event loop
- [`mod.rs`](source-map.md#core) — re-exports and public API
- [`app/mod.rs`](source-map.md#app-layer) — `App` struct and lifecycle

## Related

- [Source Map](source-map.md)
- [Flows](flows.md)
- [Tests](tests.md)
