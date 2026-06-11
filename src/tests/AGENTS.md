# src/tests/ — Test Suite

~228 test files, ~2900 tests. **All integration tests live here as flat
`*_test.rs` files**, each registered in `src/tests/mod.rs`. This is project policy
(`CONTRIBUTING.md`): inline `#[cfg(test)] mod tests { ... }` blocks at the bottom of
source files are forbidden for new integration-scope tests because they hide in IDE
outlines and grow unbounded. (Small unit tests colocated in source — e.g.
`tools/trait.rs`, `db/database.rs` — are the existing exception, not the pattern for
new feature tests.)

## Adding a Test

```bash
# 1. Create the file
$EDITOR src/tests/my_feature_test.rs

# 2. Register it in src/tests/mod.rs (alphabetical-ish neighbourhood)
#    pub mod my_feature_test;

# 3. Verify
cargo test --all-features my_feature_test
```

If the test needs internal helpers from the module under test, bump those helpers
from `fn` / `pub(super)` to `pub(crate)` so the test can reach them without
weakening the public API.

If you find an existing inline `#[cfg(test)] mod tests` while working a file, move
it into `src/tests/` as part of your change — leaving it fails review.

## File Conventions

- Name: `<area>_test.rs` (e.g. `agent_basic_test.rs`, `cron_test.rs`,
  `hashline_test.rs`). Group by subsystem prefix (`agent_*`, `provider_*`,
  `rtk_*`, `usage_*`, `session_*`).
- Import from the crate under test: `use crate::brain::agent::service::...;`.
- Async tests: `#[tokio::test]`.

## Shared Helpers

- **`agent_service_mocks.rs`** — the shared mock-provider harness. `create_test_service().await`
  returns `(AgentService, session_id)` backed by a mock provider. Use this for
  agent-loop tests instead of hitting live APIs. Imported via
  `use crate::tests::agent_service_mocks::*;`.
- **`Database::connect_in_memory()`** — the standard in-memory DB for any test
  touching persistence (28+ tests use it). Unique per call, serialized
  (`max_size=1`). Never use a file DB in tests.
- **mockito** — HTTP mocking for channels/providers that make outbound calls
  (Trello uses it).

## Running

```bash
cargo test --all-features                 # full suite (what CI runs on Linux)
cargo test --all-features my_feature_test # one file
cargo test --all-features -- --nocapture  # see println!/logs
make test                                 # = cargo test --all-features
make test-ci                              # CI profile (needs clang + mold)
```

Find which tests cover a source file you changed:

```bash
codegraph affected src/brain/tools/read.rs
```

## Notes

- CI runs tests on **Linux only** (`--all-features`); Windows/macOS get
  build-verification, not test runs.
- A few timing-sensitive tests (`rate_limiter` pacing) rely on cargo test's
  threadpool semantics and fail under nextest — the suite stays on `cargo test`.
- Benches are separate: `src/benches/{database,memory}.rs` (criterion,
  `harness = false`).
- Feature-gated tests only compile under their feature — run channel/voice tests
  with the matching `--features` set, or just use `--all-features`.
