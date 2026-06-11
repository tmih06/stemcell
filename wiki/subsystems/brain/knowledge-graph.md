# Knowledge Graph (Long-Term Memory)

**Path:** `src/brain/kg/` + `src/db/repository/knowledge_graph.rs`

An Obsidian-style markdown knowledge graph the agent reasons over: atomic notes
linked by `[[wikilinks]]`, with typed observations and relations. Files on disk
(a dedicated vault, default `~/.stemcell/vault/`) are the source of truth; the
SQLite index is rebuildable from them. Retrieval is FTS5 + bounded graph
traversal — no vector dependency.

This is distinct from the [Memory subsystem](../data/index.md) (`src/memory/`),
which is a flat FTS5 + vector log of compaction summaries. The knowledge graph
adds typed relations, backlinks, and multi-hop traversal.

## Vault Format

Each note is one atomic entity in strict Obsidian-compatible markdown:

```markdown
---
title: Rust Async
type: concept
tags: [rust, concurrency]
aliases: [async rust]
---

# Rust Async

## Observations
- [fact] Futures are lazy; nothing runs until polled #rust
- [gotcha] Holding a std Mutex across .await deadlocks (context)

## Relations
- depends_on [[Tokio Runtime]]
- contrasts_with [[Thread-per-request]]
- [[Pinning]]
```

- Folders: `concepts/`, `people/`, `projects/`, `MOCs/` (hub notes), `daily/`.
- A `.obsidian/` dir is scaffolded once so the folder opens cleanly in the GUI.
- Anti-bloat primitives: frontmatter facts, typed observation bullets, typed
  relations, and heading / `^block` anchors for slice-reads.

## Modules

| File | Role |
|---|---|
| `src/brain/kg/mod.rs` | Module root |
| `src/brain/kg/parser.rs` | Pure markdown parser — frontmatter, `[[wikilinks]]` (`#heading`, `#^block`, `\|alias`, `![[embed]]`), `#tags`, typed observations/relations |
| `src/brain/kg/resolver.rs` | Pure link resolution (name → path, case-insensitive title/stem) + anchor → line-range slicing |
| `src/brain/kg/vault.rs` | Vault path resolution, `.obsidian/` scaffold, read/write, markdown walk, slug/folder helpers |
| `src/brain/kg/sync.rs` | Filesystem → DB indexer (checksum-skip, prune, resolve), `notify` watcher, startup `spawn_indexer` |
| `src/brain/kg/traverse.rs` | Bounded-depth BFS with degree-centrality + MOC ranking and budget truncation |
| `src/db/repository/knowledge_graph.rs` | `KnowledgeGraphRepository` — the SQLite index (notes, observations, relations, FTS5) |

## Data Model

Four tables (migration `20260611000001_add_knowledge_graph.sql`), documented in
[Contracts](../../contracts.md#knowledge-graph): `kg_note` (one row per file),
`kg_note_fts` (FTS5 over title + body + observations), `kg_observation`, and
`kg_relation`. `kg_relation.to_id` is nullable — unresolved "ghost" links are
first-class rows that `resolve_dangling_links` back-fills once the target note
exists. Production connections do not enable `PRAGMA foreign_keys`, so the
repository deletes child rows explicitly rather than relying on cascades.

## Tools

All gated behind `tool-kg-*` Cargo features (umbrella `tools-kg`, in `default`).
Registered by `KnowledgeGraphModule` in `src/brain/tools/modules.rs`.

| Tool | File | Returns (anti-bloat) |
|---|---|---|
| `kg_search` | `src/brain/tools/kg_search.rs` | Entry points: `title · path · snippet · type`. No bodies. |
| `kg_read` | `src/brain/tools/kg_read.rs` | Frontmatter facts + body, or just one `#heading`/`^block` slice. |
| `kg_links` | `src/brain/tools/kg_links.rs` | Backlinks + neighbors as `relation_type → [[Target]]` lines. |
| `kg_note` | `src/brain/tools/kg_note.rs` | Creates/updates a note (surgical append, never full rewrite), reindexes. |
| `kg_context` | `src/brain/tools/kg_context.rs` | Summary-first bounded traversal: ranked titles + key facts + links. |

## Retrieval Pattern

The agent is steered (via tool descriptions and the `BRAIN_PREAMBLE_KG` section in
`src/brain/prompt_builder.rs`) toward summary-first retrieval:

```
kg_search "<query>"        → find entry-point notes (no bodies)
  → kg_context / kg_links  → expand 1–2 hops, gather facts + edges
  → kg_read "<note>"       → pull a specific note or section
kg_note                    → write durable facts as linked notes
```

## Sync & Indexing

- **Startup:** `src/cli/ui.rs` calls `sync::spawn_indexer`, which runs a full
  `reindex` then starts a `notify` watcher (mirrors `src/utils/config_watcher.rs`).
- **Change detection:** each file's sha256 checksum is compared to the stored
  one; unchanged files are skipped. Deleted files are pruned; incoming links to
  pruned notes revert to ghost state.
- **Write path:** `kg_note` writes the markdown then calls `sync::index_file` to
  reindex just that note.

## RSI Integration

The RSI cycle (`src/brain/rsi.rs`) registers `kg_search`/`kg_read`/`kg_note` and
its prompt instructs the loop to distill durable facts, decisions, and user
preferences from feedback into graph notes — searching first to avoid
duplicates. Brain files shape behavior; the graph stores knowledge.

## Configuration

`[memory].vault_dir` overrides the vault location (default
`<stemcell_home>/vault`). The knowledge graph lives in the main app SQLite DB,
not the qmd memory store, so there is no dual-store sync.

## Tests

`src/tests/kg_repository_test.rs`, `src/tests/kg_parser_test.rs`,
`src/tests/kg_resolver_test.rs`, `src/tests/kg_sync_test.rs`,
`src/tests/kg_traverse_test.rs` — see [Tests](tests.md).
