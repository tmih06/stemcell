# Coverage Manifest

Every project-owned top-level source area in `src/`. Single crate with one binary (`src/main.rs`) and one library (`src/lib.rs`).

| Source area | Coverage | Wiki page | Exclusions |
|---|---|---|---|
| `src/main.rs` | ✓ | [Entrypoints](entrypoints.md), [Flows](flows.md) | — |
| `src/lib.rs` | ✓ | [Source Map](source-map.md) | — |
| `src/cli/` | ✓ | [Subsystems/CLI](subsystems/cli/index.md), [Entrypoints](entrypoints.md) | — |
| `src/config/` | ✓ | [Contracts](contracts.md), [Subsystems/CLI](subsystems/cli/index.md) | Per-field details in 126KB `types.rs` |
| `src/db/` | ✓ | [Data Layer](subsystems/data/index.md), [Contracts](contracts.md) | Repository impl details |
| `src/migrations/` | ✓ | [Data Layer](subsystems/data/index.md), [Contracts](contracts.md) | — |
| `src/memory/` | ✓ | [Data Layer](subsystems/data/index.md), [Flows](flows.md) | Embedding model specifics |
| `src/brain/` (root) | ✓ | [Brain](subsystems/brain/index.md), [Source Map](source-map.md) | — |
| `src/brain/provider/` | ✓ | [Brain](subsystems/brain/index.md), [Contracts](contracts.md) | Per-provider impl details |
| `src/brain/agent/` | ✓ | [Brain](subsystems/brain/index.md), [Flows](flows.md) | — |
| `src/brain/agent/service/` | ✓ | [Brain](subsystems/brain/index.md), [Flows](flows.md) | Phantom language submodule |
| `src/brain/tools/` | ✓ | [Brain](subsystems/brain/index.md), [Source Map](source-map.md) | Per-tool impl details |
| `src/brain/tools/subagent/` | ✓ | [Brain/Flows](subsystems/brain/flows.md) | — |
| `src/brain/tools/browser/` | ✓ | [Brain/Flows](subsystems/brain/flows.md) | — |
| `src/brain/tools/dynamic/` | Partial | [Brain](subsystems/brain/index.md) | Runtime loading mechanics |
| `src/brain/tools/hashline/` | Partial | [Brain](subsystems/brain/index.md) | Editing semantics |
| `src/brain/mission_control/` | ✓ | [Brain/Flows](subsystems/brain/flows.md) | — |
| `src/channels/` (root) | ✓ | [Channels](subsystems/channels/index.md) | — |
| `src/channels/gateway/` | ✓ | [Channels](subsystems/channels/index.md) | — |
| `src/channels/telegram/` | ✓ | [Channels](subsystems/channels/index.md) | — |
| `src/channels/discord/` | ✓ | [Channels](subsystems/channels/index.md) | — |
| `src/channels/slack/` | ✓ | [Channels](subsystems/channels/index.md) | — |
| `src/channels/whatsapp/` | ✓ | [Channels](subsystems/channels/index.md) | — |
| `src/channels/trello/` | ✓ | [Channels](subsystems/channels/index.md) | — |
| `src/channels/voice/` | ✓ | [Channels](subsystems/channels/index.md) | — |
| `src/tui/` (root) | ✓ | [TUI](subsystems/tui/index.md) | — |
| `src/tui/app/` | ✓ | [TUI/Source Map](subsystems/tui/source-map.md) | — |
| `src/tui/render/` | ✓ | [TUI/Source Map](subsystems/tui/source-map.md) | — |
| `src/tui/onboarding/` | ✓ | [TUI](subsystems/tui/index.md) | Wizard step details |
| `src/tui/pane/` | ✓ | [TUI/Source Map](subsystems/tui/source-map.md) | — |
| `src/services/` | ✓ | [Infrastructure](subsystems/infra/index.md) | — |
| `src/a2a/` | ✓ | [Contracts](contracts.md), [Infrastructure](subsystems/infra/index.md) | JSON-RPC subset details |
| `src/cron/` | ✓ | [Flows](flows.md), [Infrastructure](subsystems/infra/index.md) | — |
| `src/logging/` | ✓ | [Infrastructure](subsystems/infra/index.md) | — |
| `src/startup/` | ✓ | [Infrastructure](subsystems/infra/index.md) | — |
| `src/rtk/` | ✓ | [Infrastructure](subsystems/infra/index.md) | — |
| `src/usage/` | ✓ | [Infrastructure](subsystems/infra/index.md) | — |
| `src/utils/` | ✓ | [Infrastructure](subsystems/infra/index.md) | Per-utility details |
| `src/patches/` | Excluded | — | Third-party wacore-binary patches |
| `src/scripts/` | ✓ | [Verification](verification.md) | — |
| `src/assets/` | Excluded | — | Binary icons, screenshots |
| `src/tests/` | Partial | [Brain/Tests](subsystems/brain/tests.md) | 228 files, not individually mapped |
| `src/benches/` | Partial | [Data/Tests](subsystems/data/tests.md) | 2 benchmark files |

## Key

| Symbol | Meaning |
|---|---|
| ✓ | Covered in wiki |
| Partial | High-level coverage, some details not documented |
| Excluded | Deliberately not documented |

## Exclusions

- Third-party patches in `src/patches/wacore-binary/` (patched WhatsApp library)
- Binary assets in `src/assets/` (icons, screenshots)
- `build_toggles.toml` at repo root — documented in [Source Map](source-map.md)
- `.github/` — CI workflows, issue/PR templates
- `wiki/` — this wiki itself
- Generated output, vendored dependencies, build artifacts, lockfiles
