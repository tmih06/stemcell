# TUI Tests

## Test Files

| File | What It Tests |
|---|---|
| [`src/tui/plan_tests.rs`](source-map.md#core) | Plan display types and rendering |

## Inline Tests

Several TUI modules contain `#[cfg(test)]` blocks with inline unit tests:

- `src/tui/events.rs` — event parsing and construction
- `src/tui/highlight.rs` — syntax highlighting edge cases
- `src/tui/markdown.rs` — markdown parse/render round-trips
- `src/tui/pane/layout.rs` — pane split calculations
- `src/tui/provider_selector.rs` — filtering and selection logic
- Various `app/` and `render/` sub-modules

## Running Tests

```sh
cargo test -p stemcell -- tui
```

## Coverage Notes

- Event system tests cover keyboard, mouse, and resize event serialization
- Plan tests cover layout, truncation, and multi-pane scenarios
- Markdown tests cover common markdown constructs (headings, lists, code blocks, tables)
- Pane tests verify equal split, custom ratio, and edge-case (zero-size) layouts
