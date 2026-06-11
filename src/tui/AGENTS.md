# src/tui/ — Terminal UI

Ratatui v0.30 + Crossterm v0.29. **Immediate mode**: the whole UI is redrawn every
frame from `App` state — no retained widget tree. Mutate `App` fields; the next
render reflects them.

## Structure

- `src/tui/` (root) — flat modules: `runner.rs` (event loop), `events.rs` (event
  types + terminal listener), `markdown.rs`/`highlight.rs` (content rendering),
  `plan.rs`, `provider_selector.rs`, `prompt_analyzer.rs`, `error.rs`,
  `onboarding_render.rs`.
- `app/` — **state + behavior**. `App` struct in `app/state.rs` (the hub, ~4000
  lines). `app/input.rs` (chat keystrokes), `app/messaging.rs` (slash-command
  dispatch + agent messaging), `app/dialogs.rs`, `app/background_session.rs`.
  Sub-dialogs are self-contained dirs (`mission_control/`, `skills_dialog/`,
  `statusline_dialog/`) each with `state.rs`/`input.rs`/`actions.rs`.
- `render/` — **pure drawing**, mirrors `app/`. `render/mod.rs::render(f, app)` is
  the single entry, matches on `app.mode`. Per-screen: `chat.rs`, `input.rs`,
  `sessions.rs`, `help.rs`, `tools.rs`, `panes.rs`, `dialogs.rs`, `palette.rs`
  (theme), `utils.rs`. Sub-dialog render dirs mirror `app/`.
- `components/` — shared reusable widgets (currently `logo.rs`). Thin.
- `onboarding/` — first-run wizard: `wizard.rs` (orchestrator), one file per step
  (`config`, `keys`, `brain`, `channels`, `voice`, `models`, `fetch`), `types.rs`
  (step enum), `input.rs`, `navigation.rs`, `helpers.rs`.
- `pane/` — tmux-style splits: `state.rs` (PaneManager), `layout.rs`, `mod.rs`.

## Event Loop & Render

`runner.rs::run(app)` → terminal setup (raw mode, alt screen, kitty keyboard,
mouse capture, panic hook restoring terminal) → `app.initialize_sync()` first
frame → `app.initialize().await` → terminal listener thread feeds events into an
mpsc channel → `run_loop`.

Each loop iteration: sync mouse-capture, flush debounced session refresh, wrap
frame in `BeginSynchronizedUpdate`/`EndSynchronizedUpdate` (DEC 2026, no flicker),
`terminal.draw(|f| render::render(f, app))` inside `catch_unwind` (render panic is
logged + shown as `app.error_message`, loop continues), check `app.should_quit`,
then `timeout(100ms, app.next_event())` and **drain + coalesce** (Ticks dropped,
MouseScroll summed/capped ±3/frame, streaming chunks batched ≤30ms).

Events: `TuiEvent` enum in `events.rs`. `app.handle_event` routes;
`handle_key_event` matches on `app.mode` and delegates to each module's
`input::handle_key`. Async work posts Uuid-tagged `TuiEvent`s via `event_sender()`
back into the loop — never block the loop.

## Adding a Screen/Dialog (use `/statusline` as the template)

To add mode `Foo`:

1. `events.rs` — add `Foo` to `enum AppMode`.
2. `app/foo_dialog/` — `mod.rs` (pub mods), `state.rs` (`FooDialogState`; add
   `pub foo_dialog: FooDialogState` to `App` and init in `App::new`), `input.rs`
   (keep a pure `decide(&mut state, key) -> KeyOutcome` for unit tests + an async
   `handle_key(app, key)` applying effects), `actions.rs` (`open(app)` sets
   `app.mode = AppMode::Foo`). Register in `app/mod.rs`.
3. `render/foo_dialog/` — `mod.rs` (`pub use dispatch::draw`), `dispatch.rs`
   (`draw(f, app, area)`). Register in `render/mod.rs` and add an
   `AppMode::Foo => foo_dialog::draw(...)` arm in `render::render`.
4. Key dispatch — add `AppMode::Foo => foo_dialog::input::handle_key(self, event).await`
   in `state.rs::handle_key_event`.
5. Slash command — add `SlashCommand { name: "/foo", … }` to `SLASH_COMMANDS` in
   `state.rs`, then a `"/foo" => foo_dialog::actions::open(self)` arm in
   `app/messaging.rs`.

A shared overlay widget instead of a full mode → add to `components/` and call
from a renderer. New chat-area element → edit `render/mod.rs` layout chunks.

## Slash Commands & Dialogs

Registry: `SLASH_COMMANDS: &[SlashCommand]` in `app/state.rs` drives autocomplete
(`word_prefix_match`, `slash_name_at`). Dispatch on submit in `app/messaging.rs`.
Dialogs are modal `AppMode` variants. Some are full-screen overlays that `Clear`
the area (Sessions, Help, MissionControl, SkillsList); others overlay the chat
shell to keep a live preview (StatusLine renders the shell then draws over a chunk).

## Onboarding

`onboarding/wizard.rs` orchestrates the step sequence (`types.rs` enum:
config→keys→brain→channels→voice→models→fetch). On first-run, `app.mode =
AppMode::Onboarding` and `app.onboarding: Option<Wizard>`; `render::render`
early-returns to `onboarding_render::render_onboarding`.

## Gotchas

- **Never cache widgets** — immediate mode. Renderers take `(f, app, area)` and
  should be side-effect-free (exception: `render_chat_shell` stashes input-area
  coords on `app` for mouse mapping).
- **Render panics are caught**, not fatal — but they blank the frame. Out-of-bounds
  Rect writes are the usual cause; clamp with `saturating_*`, guard tiny areas
  (panes skip `<3` w/h). `runner.rs` names the first `stemcell::` backtrace frame.
- **Layout indices are load-bearing**: `render/mod.rs` uses 5 fixed vertical chunks
  (chat / plan / queue / input / status bar); `Length(0)` collapses unused.
  `debug_assert!(chunks.len() >= 5)`.
- **Keep `decide()` pure** in dialog `input.rs` (mutates only its state, returns
  `KeyOutcome`) so keystroke logic is unit-testable without an `App`.
- **Config-backed dialogs**: keep field metadata in one ordered table (see
  `statusline_dialog::FIELDS` with label/key/get/set fn-pointers) so the list,
  config struct, and persistence key stay in sync; persist via `Config::write_key`.
- Async: don't `.await` long ops in `handle_event`; post events. Session refreshes
  debounced 500ms.

## Tests

**New tests go in `src/tests/<area>_test.rs`** (project policy — see
`src/tests/AGENTS.md`), not new inline `#[cfg(test)] mod tests` blocks. The TUI has
many existing inline tests; treat them as **style references**, not the pattern to
extend. The cleanest model for a pure, `App`-free keystroke test is
`app/statusline_dialog/input.rs` (`decide` tests); also `src/tui/plan_tests.rs`.
Existing inline tests live in `app/input.rs`, `app/state.rs`, `events.rs`,
`markdown.rs`, `highlight.rs`, `render/mod.rs`, `render/utils.rs`,
`render/statusline_dialog/dispatch.rs`, `onboarding/mod.rs` — `render/mod.rs`
`#[cfg(test)]`-re-exports internal fns so a test file can reach them. Run:
`cargo test tui`.
