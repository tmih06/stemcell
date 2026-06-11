# TUI Flows

## Event Loop

```
runner.rs::run()
  ↓
events.rs::poll_event()        ← keyboard, mouse, resize
  ↓
app/mod.rs::App::update()      ← state machine transition
  ↓
render/mod.rs::render()        ← dispatch to panel renderers
  ↓
crossterm::queue!()             ← write to terminal
```

- Main loop in [`src/tui/runner.rs`](source-map.md#core)
- Event polling in [`src/tui/events.rs`](source-map.md#core)

## Input Handling

```
Keyboard/mouse event
  ↓
events.rs → key code or mouse action
  ↓
app/input.rs → AppInput buffer/state
  ↓
app/messaging.rs → message dispatch
  ↓
app/state.rs → state transition
```

- Input state: [`src/tui/app/input.rs`](source-map.md#app-layer)
- Message dispatch: [`src/tui/app/messaging.rs`](source-map.md#app-layer)

## Split Pane Layout

```
User drags divider or sends resize command
  ↓
events.rs → resize event
  ↓
pane/state.rs → update pane sizes
  ↓
pane/layout.rs → recalculate Rect allocations
  ↓
render/mod.rs → render each pane's content
```

- Pane management: [`src/tui/pane/`](source-map.md#pane-system)
- Rendering: [`src/tui/render/panes.rs`](source-map.md#rendering)

## Onboarding Wizard

```
First run detected → launch wizard
  ↓
onboarding/wizard.rs → orchestrator
  ↓
Step sequence:
  onboarding/config.rs    → configure provider defaults
  onboarding/keys.rs      → enter API keys
  onboarding/brain.rs     → select brain/backend
  onboarding/channels.rs  → configure channels
  onboarding/voice.rs     → voice/TTS setup
  onboarding/models.rs    → model selection
  onboarding/fetch.rs     → fetch initial data
  ↓
onboarding_render.rs → render current step
  ↓
Complete → transition to main chat TUI
```

- Wizard: [`src/tui/onboarding/`](source-map.md#onboarding)
- Rendering: [`src/tui/onboarding_render.rs`](source-map.md#core)

## Mission Control

```
Mission Control mode activated
  ↓
app/mission_control/state.rs → manage RSI data
  ↓
Render panels:
  render/mission_control/inbox.rs      → RSI inbox messages
  render/mission_control/activity.rs   → activity feed
  render/mission_control/schedule.rs   → cron schedule
  ↓
Item selected → detail popup → render/mission_control/layout.rs
```

- App logic: [`src/tui/app/mission_control/`](source-map.md#app-layer)
- Render: [`src/tui/render/mission_control/`](source-map.md#rendering)

## Skills Dialog

```
Skills dialog triggered
  ↓
app/skills_dialog/state.rs → load skill list
  ↓
render/skills_dialog/card.rs     → card view per skill
render/skills_dialog/dispatch.rs → render selection
  ↓
app/skills_dialog/input.rs → navigate/select
  ↓
app/skills_dialog/actions.rs → confirm selection
  ↓
Return selected skill to chat context
```

- App logic: [`src/tui/app/skills_dialog/`](source-map.md#app-layer)
- Render: [`src/tui/render/skills_dialog/`](source-map.md#rendering)
