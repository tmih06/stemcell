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
| `src/brain/kg/sync.rs` | Filesystem → DB indexer (checksum-skip, prune, resolve), `notify` watcher, startup `spawn_indexer`, watcher-suppression gate |
| `src/brain/kg/traverse.rs` | Bounded-depth BFS with degree-centrality + MOC ranking and budget truncation |
| `src/brain/kg/compose.rs` | Pure note-composition helpers (`build_note`, `insert_bullets`, observation/relation bullets, `resolve_note_rel`), shared by `kg_note` and the review gate |
| `src/brain/kg/git_review.rs` | `GitRepo` — git shell-out wrapper (init/commit/worktree/diff/merge/log/show/revert/reset) + pure diff-shortstat & log parsers |
| `src/brain/kg/review.rs` | Review-gate orchestration (`queue_batch`/`approve`/`decline`/`revert_last`/`restore`/`list_pending`/`batch_diff`/`log`/`show`) — the single service both `kg_remember` and the `/kg` TUI call |
| `src/db/repository/knowledge_graph.rs` | `KnowledgeGraphRepository` — the SQLite index (notes, observations, relations, FTS5) |
| `src/db/repository/kg_pending_batch.rs` | `KgPendingBatchRepository` — the durable review queue (`kg_pending_batch` rows) |

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
| `kg_remember` | `src/brain/tools/kg_remember.rs` | Review-gated alternative to `kg_note`: seals a multi-note batch onto a branch and parks it in the queue. Registered (in place of `kg_note`) when the review gate is active — `approve-only` policy or `kg_review_enabled`. |

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
- **Incremental watcher:** the watcher does not full-reindex on every edit. It
  accumulates the changed paths across a 500ms debounce window, classifies each
  via `Vault::classify_path` (`PathClass::Note`/`Other`/`Ignore`), then calls
  `sync::sync_paths` to touch only those notes — indexing paths still on disk,
  pruning those gone. `.obsidian/`-style noise is `Ignore`d so it triggers no
  work. A folder-scope change (`PathClass::Other` — a dir rename/delete moves
  child notes with no per-note event) falls back to a full `reindex`.
- **Prune is snapshot-scoped (no TOCTOU):** `reindex` computes the prune set from
  the `(path, checksum)` snapshot taken *before* the filesystem walk
  (`existing − seen`) and deletes those exact paths via `repo.prune_paths`. A note
  committed concurrently (e.g. a `kg_note` write landing mid-walk) is absent from
  that snapshot, so it can never be pruned as collateral.
- **Write path:** `kg_note` writes the markdown then calls `sync::index_file` to
  reindex just that note.

## Git Review Gate (optional)

The review gate is **driven by the `/approve` permission policy**
(`agent.approval_policy`): when the agent is in `approve-only` mode — i.e. you're
already approving each tool call — its knowledge-graph memory writes are routed
through the `/kg` queue too. Under `auto-session` / `auto-always` (the default),
`kg_note` writes straight to disk + index as before. The decision lives in
[`Config::kg_review_active`](../../contracts.md#config-structure) so registration
(`modules.rs`) and startup repo-init (`sync.rs`) can't drift.

Two `[memory]` flags override the policy for users who want a fixed behavior:

- `kg_review_enabled` — forces the gate **on** regardless of policy: the main
  agent's `kg_note` is **replaced** by `kg_remember` (`modules.rs`), so it can no
  longer write straight to long-term memory. RSI keeps its own ungated `kg_note`
  (its apply/reject loop is its gate). Implies git backing.
- `kg_git_enabled` — forces git backing **on** even when the gate is off (plain
  versioning without the queue): the vault becomes a git repo, `spawn_indexer`
  calls `review::ensure_repo` at startup (idempotent `git init` + `.gitignore`
  [`.obsidian/`, `.trash/`] + `.gitattributes` [`*.md merge=union`] + initial
  commit), and every approved state is restorable via `/kg log` / `/kg restore`.

Git backing turns on whenever the gate is active (the queue needs a repo to stage
onto), so `approve-only` alone is enough to get the full versioned-review flow
with no `[memory]` config at all.

**Staging model.** A `kg_remember` call seals all its notes onto a `kg/batch/<id>`
branch checked out in a worktree at `<vault>/../.kg-staging/<id>` — a *sibling* of
the vault root, deliberately **outside** the watched tree, so composing a batch
trips zero `notify` events. The worktree shares main's `.git`, so the branch is
visible for diff/merge/log. A `kg_pending_batch` row (status `pending`) records
the branch, base sha, summary, and diff shortstat.

**Review.** `/kg` opens a full-screen TUI (Mission Control pattern): left pane
lists pending/conflicted batches + recent vault history, right pane shows the
batch diff. Per-batch `a` = approve, `d` = decline; `r` in the log view arms a
two-key restore confirm.

- **Approve** → `git merge --no-ff` the branch into main. `*.md merge=union`
  auto-merges non-conflicting appends; pre-commit `insert_bullets` dedup keeps
  union from duplicating bullets. On success: one authoritative `reindex`, mark
  `approved` (store merge sha), remove worktree + branch. A true same-line
  conflict aborts the merge, marks the batch `conflicted` (branch kept for manual
  resolution), and leaves the index untouched.
- **Decline** → drop branch + worktree, mark `declined`. Main is untouched.
- **Revert** → `git revert -m 1` the last approved merge, then reindex.
- **Restore** → `git reset --hard <sha>` to any historical commit, then reindex
  (destructive; TUI-confirmed).

**Watcher suppression.** Merge/revert/restore are the only ops that mutate the
watched tree. Each runs under a `sync::suppress_begin()` RAII guard (process-global
`AtomicU64` depth + generation counters) followed by exactly one `sync::reindex` —
the sole authoritative index update. The watcher's debounce loop captures the
generation after its first event and drops the burst if `suppressed()` or the
generation moved, so correctness rests on the counter, not on debounce timing.

Service layer: `src/brain/kg/review.rs` is the single entry point both
`kg_remember` and the `/kg` TUI call, so git + queue + suppression + reindex
policy lives in one place. The git plumbing is `src/brain/kg/git_review.rs`
(`GitRepo`); the queue is `src/db/repository/kg_pending_batch.rs`
(`kg_pending_batch`, migration `20260613000001_add_kg_pending_batch.sql`).

## RSI Integration

The RSI cycle (`src/brain/rsi.rs`) registers `kg_search`/`kg_read`/`kg_note` and
its prompt instructs the loop to distill durable facts, decisions, and user
preferences from feedback into graph notes — searching first to avoid
duplicates. Brain files shape behavior; the graph stores knowledge.

## Configuration

`[memory].vault_dir` overrides the vault location (default
`<stemcell_home>/vault`). The knowledge graph lives in the main app SQLite DB,
not the qmd memory store, so there is no dual-store sync. The review gate is
driven by `agent.approval_policy` (`approve-only` engages it); `[memory].kg_git_enabled`
and `[memory].kg_review_enabled` (both default false) override the policy to force
git versioning or the gate on — see [Git Review Gate](#git-review-gate-optional).

## Tests

`src/tests/kg_repository_test.rs`, `src/tests/kg_parser_test.rs`,
`src/tests/kg_resolver_test.rs`, `src/tests/kg_sync_test.rs`,
`src/tests/kg_traverse_test.rs`, `src/tests/kg_note_test.rs`,
`src/tests/kg_registration_test.rs`, `src/tests/kg_git_review_test.rs`,
`src/tests/kg_pending_batch_test.rs` — see [Tests](tests.md).
