# Contributing Agent Rules

These rules apply to all AI agents modifying the StemCell codebase.

## Before Modifying Code

1. **Read relevant wiki pages** — at minimum the [index](index.md), [coverage-manifest](coverage-manifest.md), and the subsystem's source map.
2. **Inspect high-risk files** listed in [agent-quickstart.md](agent-quickstart.md) if the change touches agent, provider, config, DB, or channel code.
3. **Understand the flow** — read [flows.md](flows.md) for the relevant execution path.

## Wiki Update Requirements

You **must** update wiki pages when making durable changes to:

| Change type | Required wiki updates |
|---|---|
| Architecture / behavior / flow | [Flows](flows.md), [Architecture](architecture/index.md), [Architecture Boundaries](architecture/boundaries.md) |
| Commands / build / verification | [Entrypoints](entrypoints.md), [Verification](verification.md) |
| Configuration keys or structure | [Contracts](contracts.md), [Source Map](source-map.md) |
| Test structure or coverage | [Coverage Manifest](coverage-manifest.md) |
| Public API (traits, types, interfaces) | [Contracts](contracts.md) |
| File responsibility (new, renamed, moved, removed files) | [Source Map](source-map.md) |
| Schema or DB migrations | [Contracts](contracts.md) |
| Tool / Provider / Channel registration | [Change Map](change-map.md) |
| Workflows or CI | [Flows](flows.md), [Verification](verification.md) |

## Coverage Manifest Updates

Update [coverage-manifest.md](coverage-manifest.md) when source areas are:

- Added (new module directory with public items)
- Removed (deleted module)
- Renamed or relocated
- Excluded from coverage tracking
- Reorganized (split, merged, regrouped)

## Stale Wiki Detection

**Block completion** when relevant wiki locator information is stale. Examples:

- A file path referenced in the wiki no longer exists
- A module mentioned in the wiki has been renamed
- A flow described in [flows.md](flows.md) no longer matches the source
- A contract described in [contracts.md](contracts.md) is outdated
- A command listed in [verification.md](verification.md) no longer works

When blocking, note the discrepancy and propose the fix. Do not proceed with code changes until wiki accuracy is restored.
