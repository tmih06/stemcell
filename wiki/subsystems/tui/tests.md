# TUI Tests

## Test Files

| File | What It Tests |
|---|---|
| [`src/tui/plan_tests.rs`](source-map.md#core) | Plan display types and rendering |
| `src/tests/model_search_test.rs` | Model search: query normalization, multi-term matching, highlight spans |
| `src/tests/provider_selector_test.rs` | Provider visibility gating + `/models` multi-term filtering |
| `src/tests/tui_jump_to_latest_test.rs` | "Jump to latest" toast counter: new-message count after the scroll anchor |

## Inline Tests

Several TUI modules contain `#[cfg(test)]` blocks with inline unit tests:

- `src/tui/events.rs` — event parsing and construction
- `src/tui/highlight.rs` — syntax highlighting edge cases
- `src/tui/markdown.rs` — markdown parse/render round-trips
- `src/tui/pane/layout.rs` — pane split calculations
- Various `app/` and `render/` sub-modules

`src/tui/provider_selector.rs` and `src/tui/model_search.rs` tests now live in
`src/tests/` (see the table above), per `src/tests/AGENTS.md`.

## Running Tests

```sh
cargo test -p stemcell -- tui
```

## Coverage Notes

- Event system tests cover keyboard, mouse, and resize event serialization
- Plan tests cover layout, truncation, and multi-pane scenarios
- Markdown tests cover common markdown constructs (headings, lists, code blocks, tables)
- Pane tests verify equal split, custom ratio, and edge-case (zero-size) layouts
