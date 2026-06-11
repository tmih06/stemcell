# Agent Quickstart

Before changing any source code, you **must**:

1. Read [index.md](index.md) — understand the project shape
2. Read [coverage-manifest.md](coverage-manifest.md) — know which areas are tracked
3. Read the relevant subsystem source map — find the files that own the behavior you're changing
4. Read [contributing-agent-rules.md](contributing-agent-rules.md) — wiki update obligations

## High-Risk Files

These files have broad impact. Change with extreme care:

| File | Risk |
|------|------|
| `src/brain/agent/service/tool_loop.rs` | Core agent loop — 253KB, orchestrates all tool execution |
| `src/brain/provider/factory.rs` | Provider instantiation — 57KB, all LLM backend wiring |
| `src/brain/tools/registry.rs` | Tool registration — every tool must be registered here |
| `src/config/types.rs` | Config struct — 126KB, every subsystem depends on it |
| `src/db/database.rs` | Database pool & init — affects all persistence |
| `src/channels/gateway/bus.rs` | Channel gateway — the single inbound→agent→outbound bus and surface lifecycle |

## Verification Commands

```bash
# Full test suite (all features)
cargo test --all-features

# Lint
cargo clippy --all-features

# Build
cargo build --all-features

# Wiki integrity
make docs-coverage   # links, source refs, formatting

# Full local verification gate
make verify    # fmt-check + lint + test + doc + docs-coverage
```

## Wiki Pages That Must Change with Durable Source Changes

| When you change... | Update these wiki pages |
|-------------------|------------------------|
| File responsibility (new/renamed/moved file) | [Source Map](source-map.md) |
| Architecture, flow, boundary | [Flows](flows.md), [Architecture](architecture/index.md), [Architecture Boundaries](architecture/boundaries.md) |
| Commands, build, verification | [Verification](verification.md), [Entrypoints](entrypoints.md) |
| Configuration structure | [Contracts](contracts.md), [Source Map](source-map.md) |
| Database schema | [Contracts](contracts.md) |
| Public API (Provider trait, Tool trait, A2A types) | [Contracts](contracts.md) |
| Test structure | [Coverage Manifest](coverage-manifest.md) |
| Tool/Provider/Channel registration pattern | [Change Map](change-map.md) |
| Feature flags or build system | [Coverage Manifest](coverage-manifest.md), `Cargo.toml` docs |

## Stale Wiki Detection

If source code contradicts a wiki page, **the source is truth**. Update the wiki page and note the discrepancy.
