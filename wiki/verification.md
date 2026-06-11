# Verification

All verification commands, sourced from `Makefile` and `Cargo.toml`.

## Make Targets

| Command | What it does |
|---------|-------------|
| `make build` | Build dev binary from `build_toggles.toml` |
| `make build-ci` | Build all features with CI profile |
| `make build-release` | Build release binary from `build_toggles.toml` |
| `make build-no-default` | Build with no default features |
| `make build-minimal` | Build core tools only, no channels (`build.sh minimal`) |
| `make build-chatbot` | Build no tools, pure chatbot mode |
| `make build-telegram` | Build core tools + Telegram |
| `make build-headless` | Build full tools, no channels |
| `make check` | Fast type-check (`cargo check --all-targets --all-features`) |
| `make fmt` | Format code (`cargo fmt --all`) |
| `make fmt-check` | Check formatting |
| `make lint` | Clippy with `-D warnings` |
| `make test` | Full test suite (`cargo test --all-features`) |
| `make test-ci` | CI test profile (clang + mold linker) |
| `make doc` | Build docs (`cargo doc --no-deps --document-private-items`) |
| `make audit` | `cargo audit` |
| `make coverage` | `cargo tarpaulin` (cobertura.xml) |
| `make deny` | `cargo deny check advisories licenses sources` |
| `make typos` | `typos` spell check |
| `make secrets` | `gitleaks` secret scan |
| `make msrv` | Verify MSRV (1.91) |
| `make docs-coverage` | Verify wiki integrity (links, source refs, format) |
| `make verify` | `fmt-check + lint + test + doc + docs-coverage` |
| `make ci` | Full CI suite (verify + build-ci + audit + coverage + deny + typos + secrets + msrv) |
| `make run` | Run TUI with dev features |
| `make install` | `cargo install` from source |
| `make clean` | `cargo clean` |
| `make setup` | Install system prerequisites |
| `make build-profiles` | List available build profiles |

## Cargo Commands (Direct)

| Command | Purpose |
|---------|---------|
| `cargo build --all-features` | Build all features |
| `cargo test --all-features` | Run all tests |
| `cargo clippy --all-features` | Lint all features |
| `cargo fmt --all --check` | Format check |
| `cargo doc --no-deps --document-private-items` | Generate docs |
| `cargo audit` | Security audit |
| `cargo tarpaulin --out Xml` | Code coverage |
| `cargo deny check advisories licenses sources` | License/advisory check |
| `cargo +1.91 build --locked --all-features` | MSRV check |

## CI Workflows

GitHub Actions in `.github/workflows/`:

- **CI**: push → lint → build → test → audit → coverage → deny → typos → secrets → docs-coverage → msrv → release (on tag)

## Profile-Specific Builds

| Profile | Cargo flag | Use case |
|---------|-----------|----------|
| dev | (default) | Local development |
| release | `--release` | Production binary |
| release-small | `--profile release-small` | Size-optimized binary |
| ci | `--profile ci` | CI pipeline (thin LTO, 16 codegen units) |

## Note

- `make` targets use `build_toggles.toml` for feature selection, not `--all-features`.
- CI uses `--all-features` for coverage.
- Always verify with both `make verify` (local gate) and the relevant individual commands for the area you changed.
