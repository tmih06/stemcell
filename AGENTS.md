# StemCell — Agent Instructions

StemCell (v0.3.35) is a modular Rust CLI/TUI shell for LLMs: a pluggable agent with
30+ tools, multi-channel messaging (Telegram, Discord, Slack, WhatsApp, Trello),
long-term memory (FTS5 + vector), cron jobs, A2A protocol, voice I/O, and recursive
self-improvement (RSI). Rust edition 2024, MSRV 1.91, MIT.

## Golden Rules

1. **Read the wiki before changing code.** Start at `wiki/index.md`, then the
   subsystem page for the area you're touching. The wiki is a source locator —
   find the right file there, then verify behavior in the source before editing.
2. **Use codegraph for code search** (not raw grep for symbols). See below.
3. **Use rtk** — bash commands are auto-proxied for token savings; don't fight it.
4. **Verify with `--all-features`.** Tools/channels/providers are `#[cfg]`-gated;
   a default build only compiles a subset. Only `--all-features` exercises every
   branch. CI does too.
5. **Tests live in `src/tests/`**, one `*_test.rs` file per area, registered in
   `src/tests/mod.rs`. No new inline `#[cfg(test)] mod tests` blocks in source.
6. **Atomic commits, Conventional Commits, no `Co-Authored-By`** (project policy).
7. **Update the wiki in the same change** when durable behavior shifts.

## Where To Work (sub-guides — read only what you need)

| Area | Guide | Source |
|------|-------|--------|
| Agent core, tools, providers, RSI | `src/brain/AGENTS.md` | `src/brain/` |
| TUI (ratatui) | `src/tui/AGENTS.md` | `src/tui/` |
| Messaging channels | `src/channels/AGENTS.md` | `src/channels/` |
| DB, memory, migrations | `src/db/AGENTS.md` | `src/db/`, `src/memory/`, `src/migrations/` |
| Config & secrets | `src/config/AGENTS.md` | `src/config/` |
| Build, CLI, A2A, cron, infra | `src/cli/AGENTS.md` | `src/cli/`, `src/a2a/`, build files |
| Tests | `src/tests/AGENTS.md` | `src/tests/` |

Load the sub-guide for your subsystem instead of reading this whole tree. Each
sub-guide is self-contained: structure, how-to-add-X, gotchas, where tests live.

## codegraph — always use for code search

The repo is indexed (`.codegraph/codegraph.db`, 604 files, 12k nodes). Prefer it
over grep when locating symbols or tracing relationships:

```bash
codegraph query "AgentService" -k struct      # find a symbol (kind: function/struct/enum/trait/method)
codegraph callers run_tool_loop                # who calls this
codegraph callees run_tool_loop                # what this calls
codegraph impact ToolResult -d 2               # blast radius of changing a symbol
codegraph affected src/brain/tools/read.rs     # which test files cover a changed file
codegraph sync                                 # refresh index after edits
```

Use `impact` before touching a high-risk symbol, and `affected` to find which
tests to run for your change.

## rtk — Rust Token Killer

Bash commands are auto-rewritten through `rtk` by a hook (transparent, 0 overhead),
compressing output 60–90%. Just run `git status`, `cargo …`, etc. normally.
Meta commands you call directly: `rtk gain` (savings), `rtk discover`,
`rtk proxy <cmd>` (bypass filtering for debugging). The in-repo `src/rtk/` module
mirrors this for the agent's own bash tool (feature `rtk`).

## Build & Run

```bash
make build            # dev build via build_toggles.toml (Python feature resolver)
make run              # launch the TUI (ARGS='-p hermes' to pass flags)
make run ARGS='/sessions'
cargo run --all-features            # alt dev loop; reads ~/.stemcell/ like the install
```

**Never hand-roll `cargo build --features …`.** `make build` runs
`src/scripts/tool_features.py` against `build_toggles.toml` and sets
`STEMCELL_EXPECTED_FEATURES`, which `build.rs` cross-checks and **panics** on
mismatch. Feature definitions are duplicated in `Cargo.toml`, `build.rs`, and
`tool_features.py` — keep all three in sync. Build profiles (`build-profiles.toml`
via `./build.sh <profile>`): `minimal`, `chatbot`, `headless-agent`, etc.

## Verify (run before every PR)

```bash
make verify    # fmt-check + lint + test + doc + docs-coverage (the local gate)
```

Or the exact CI commands individually:

```bash
cargo fmt --all -- --check
cargo clippy --lib --bins --tests --all-features -- -D warnings   # -D warnings: CI fails on any
cargo test --all-features
make docs-coverage    # wiki integrity: links, source refs, formatting (src/scripts/check-wiki.sh)
```

`cargo clippy` is the trusted lint pass — `cargo check` misses the rules CI
enforces. Iterate with clippy so you don't burn a CI run. CI also runs: cross-platform
build (Win/macOS), cargo-audit, cargo-deny, typos, gitleaks, MSRV 1.91, tarpaulin coverage.

## High-Risk Files (change with extreme care)

| File                                   | Why                                                |
| -------------------------------------- | -------------------------------------------------- |
| `src/brain/agent/service/tool_loop.rs` | Core agent loop (~4900 lines)                      |
| `src/brain/provider/factory.rs`        | All provider wiring; fragile fallback wrapping     |
| `src/brain/tools/modules.rs`           | Tool registration source of truth                  |
| `src/config/types.rs`                  | 126KB config struct; every subsystem depends on it |
| `src/db/database.rs`                   | Pool, pragmas, migration registration              |
| `src/channels/manager.rs`              | Channel lifecycle; repeated cfg lists              |

For `types.rs` use `grep -n "pub struct"` / line-offset reads — never read it whole.

## Commit Message Standard

Follow [Conventional Commits](https://www.conventionalcommits.org/). Format:

```
<type>(<optional scope>): <imperative subject, ≤70 chars, no trailing period>
<blank line>
<body: WHY the change is needed and what would break if reverted — not the diff>
<blank line>
<optional footer: "Closes #123", "Refs #45", "BREAKING CHANGE: …">
```

**Types**: `feat` (new capability), `fix` (bug fix), `refactor` (no behavior
change), `perf` (with benchmark), `chore` (build/deps/tooling), `docs`, `test`,
`style` (formatting only). Scope is the subsystem: `feat(tui):`, `fix(channels):`,
`fix(provider):`, `chore(deps):`.

Examples (from this repo's history):

```
feat(tui): add /statusline dialog to toggle status bar fields
fix: prevent duplicate message rendering for CLI providers
refactor: simplify tool loop iteration tracking
chore: cargo fmt
```

Rules:

- **Subject** imperative mood ("add", not "added"/"adds"), ≤70 chars, lowercase
  after the colon, no period.
- **Body** explains the *why*, not the *what* — the diff already shows what
  changed. Answer "why was this wrong?" and "what breaks if we revert?". Wrap ~72
  cols. Omit only for trivial, self-evident commits.
- **Atomic**: one logical change per commit. Don't bundle `cargo fmt` drift or
  renames with logic — format-only churn goes in its own `chore: cargo fmt`
  commit, mechanical renames separate from behavioral changes.
- Split test additions from production fixes *only* if the test compiles against
  the unfixed code; otherwise commit them together so the test proves the fix.
- Add `[skip ci]` to docs/chore/non-functional commits — but **never** to a
  release commit (it skips the release workflow too).
- **Never** add `Co-Authored-By` lines (project policy).

## PRs

- Branch from `main` (never `master`). Push to a feature branch, open a PR with
  `gh pr create`. Fill `.github/PULL_REQUEST_TEMPLATE.md`: description, linked
  issue (`Closes #N`), type, checklist, testing notes, screenshots for UI.
- Feature PRs need an approved issue first. No stub/placeholder code
  (`todo!()`, empty impls) — closed on sight. See `CONTRIBUTING.md`.
- All three CI gates must pass locally before submitting: `cargo fmt --all --
  --check`, `cargo clippy … -D warnings`, `cargo test --all-features`.

## Wiki Update Obligations

When code changes alter durable behavior, update the affected wiki pages in the
same task:

| Changed…                                    | Update                                        |
| ------------------------------------------- | --------------------------------------------- |
| File responsibility (new/renamed/moved)     | `wiki/source-map.md` + subsystem source-map   |
| Architecture/flow/boundary                  | `wiki/flows.md`, `wiki/architecture/*`        |
| Commands/build/verification                 | `wiki/verification.md`, `wiki/entrypoints.md` |
| Config structure / DB schema / public trait | `wiki/contracts.md`                           |
| Source area added/removed/renamed           | `wiki/coverage-manifest.md`                   |
| Tool/Provider/Channel registration pattern  | `wiki/change-map.md`                          |

If source contradicts the wiki, **source is truth** — fix the wiki and note it.
Don't mark work complete with stale wiki locators. Full rules:
`wiki/contributing-agent-rules.md`.

## Conventions

- Files `snake_case.rs`; structs/enums `PascalCase`; fns/vars `snake_case`;
  consts `SCREAMING_SNAKE_CASE`.
- Errors: `anyhow::Result` for app code, `thiserror` for typed errors.
- Async: `tokio` — never block in async fns (use `spawn_blocking` for DB/CPU work).
- Secrets: type as `config::secrets::SecretString` (redacts on serialize); keep
  in `keys.toml`; never log `expose_secret()`.
- Minimal diffs. Comments explain *why*, not *what*.
```

