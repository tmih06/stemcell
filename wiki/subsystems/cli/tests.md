# CLI & Config — Tests

## Running Tests

```bash
# All tests
cargo test

# All features (includes all CLI subcommands)
cargo test --all-features

# CLI-specific tests (if test binary is split)
cargo test -p stemcell cli
cargo test -p stemcell config
```

## Test Strategy

### CLI arg parsing

Clap's derive API provides compile-time validation of argument structure. No runtime tests needed for basic parsing — the derive macros verify:
- Required args are present
- Value enums match expected variants
- Subcommand dispatch is exhaustive

### Command handlers

`commands.rs` handlers are tested by constructing a `Config` directly and calling each `cmd_*` function. Tests verify:
- Status/doctor output contains expected sections
- `cmd_run` produces a response
- `cmd_init` creates config files
- `cmd_db` operations succeed/error appropriately

### Config loading

Config tests cover:
- Deserialization of each `config.toml.example` field
- `keys.toml` separation and merge
- `SecretString` zeroize-on-drop behavior
- Profile registry load/save/create/list/delete
- Validation failures in `health.rs`

### Config hot-reload

Integration tests validate the `notify`-based watcher triggers config re-read and broadcasts updates to the channel gateway (surface reconcile) and agents.

## CI

- `cargo test --all-features` runs on every push/PR
- Config example files are tested for valid TOML deserialization
