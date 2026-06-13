//! Filesystem → SQLite index synchronization for the knowledge-graph vault.
//!
//! The vault on disk is the source of truth; this module rebuilds the
//! [`KnowledgeGraphRepository`] index from it. [`reindex`] does a full pass with
//! sha256/mtime change-detection (unchanged files are skipped), prunes notes for
//! deleted files, and back-fills resolved links. A `notify` watcher keeps the
//! index live, and [`spawn_indexer`] wires the initial reindex + watcher at
//! startup.

use super::parser;
use super::resolver::filename_stem;
use super::vault::{self, PathClass, Vault};
use crate::config::Config;
use crate::db::{KnowledgeGraphRepository, NoteUpsert, ObservationInput, Pool, RelationInput};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

/// Outcome counters for a sync pass.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncStats {
    pub indexed: usize,
    pub skipped: usize,
    pub pruned: usize,
    pub resolved: usize,
}

// --- watcher suppression gate ---
//
// Git review operations (merge, revert, reset) mutate the watched vault working
// tree, which would trip the `notify` watcher into reindexing a tree that is
// mid-rewrite. The gate lets the caller suppress watcher-driven reindexes for
// the duration of a git op, then do one authoritative `reindex` afterward.
//
// Two counters make this timing-independent: `DEPTH` (>0 while any op holds a
// guard) and `GEN` (bumped on every guard drop). The watcher captures `GEN` when
// a debounce burst starts and, before acting, drops the burst if either the gate
// is currently held OR `GEN` moved — so a burst that began before/during an op
// (even one that finished within the debounce window) is always discarded.

static SUPPRESS_DEPTH: AtomicU64 = AtomicU64::new(0);
static SUPPRESS_GEN: AtomicU64 = AtomicU64::new(0);

/// RAII guard that suppresses watcher reindexes while held. Drop bumps the
/// generation counter so any burst overlapping the guarded op is discarded.
#[must_use = "the gate is released when the guard is dropped"]
pub struct SuppressGuard;

/// Begin suppressing watcher-driven reindexes. The returned guard releases the
/// suppression (and bumps the generation) when dropped.
pub fn suppress_begin() -> SuppressGuard {
    SUPPRESS_DEPTH.fetch_add(1, Ordering::SeqCst);
    SuppressGuard
}

impl Drop for SuppressGuard {
    fn drop(&mut self) {
        SUPPRESS_DEPTH.fetch_sub(1, Ordering::SeqCst);
        SUPPRESS_GEN.fetch_add(1, Ordering::SeqCst);
    }
}

/// True while at least one [`SuppressGuard`] is held.
pub fn suppressed() -> bool {
    SUPPRESS_DEPTH.load(Ordering::SeqCst) > 0
}

/// Current suppression generation — bumped each time a guard is dropped.
pub fn suppress_gen() -> u64 {
    SUPPRESS_GEN.load(Ordering::SeqCst)
}

/// Full reindex pass: walk the vault, (re)index changed files, prune deleted
/// notes, and resolve dangling links. Unchanged files (matching checksum) are
/// skipped. Scaffolds the vault first so a missing vault is created, not an error.
pub async fn reindex(vault: &Vault, repo: &KnowledgeGraphRepository) -> Result<SyncStats> {
    vault
        .ensure_scaffold()
        .with_context(|| format!("Failed to scaffold vault at {:?}", vault.root()))?;

    let existing: std::collections::HashMap<String, String> =
        repo.all_paths_with_checksums().await?.into_iter().collect();

    let mut stats = SyncStats::default();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for abs in vault.list_markdown() {
        let Some(rel) = vault.relative(&abs) else {
            continue;
        };
        let bytes = match std::fs::read(&abs) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("KG sync: failed to read {:?}: {e}", abs);
                continue;
            }
        };
        seen.insert(rel.clone());

        let checksum = checksum_hex(&bytes);
        if existing.get(&rel).map(String::as_str) == Some(checksum.as_str()) {
            stats.skipped += 1;
            continue;
        }

        let content = String::from_utf8_lossy(&bytes).into_owned();
        let meta = std::fs::metadata(&abs).ok();
        let (note, observations, relations) =
            build_inputs(&rel, &content, &checksum, meta.as_ref());
        repo.index_note(note, observations, relations)
            .await
            .with_context(|| format!("Failed to index {rel}"))?;
        stats.indexed += 1;
    }

    // Prune from the pre-walk snapshot, not live DB state: a note committed
    // concurrently (e.g. a `kg_note` write) isn't in `existing`, so it can never
    // be a prune candidate — this closes the walk/prune TOCTOU window.
    let doomed: Vec<String> = existing
        .keys()
        .filter(|p| !seen.contains(*p))
        .cloned()
        .collect();
    stats.pruned = repo.prune_paths(&doomed).await?;
    stats.resolved = repo.resolve_dangling_links().await?;
    Ok(stats)
}

/// Index a single note by vault-relative path (read → parse → upsert → resolve).
/// Used by the `kg_note` tool after a surgical write. Returns the note id.
pub async fn index_file(vault: &Vault, repo: &KnowledgeGraphRepository, rel: &str) -> Result<i64> {
    let abs = vault.note_path(rel);
    let bytes = std::fs::read(&abs).with_context(|| format!("Failed to read {rel}"))?;
    let checksum = checksum_hex(&bytes);
    let content = String::from_utf8_lossy(&bytes).into_owned();
    let meta = std::fs::metadata(&abs).ok();
    let (note, observations, relations) = build_inputs(rel, &content, &checksum, meta.as_ref());
    let id = repo
        .index_note(note, observations, relations)
        .await
        .with_context(|| format!("Failed to index {rel}"))?;
    // Scoped resolve: only this note's own ghost links plus ghosts elsewhere that
    // now point at it — far cheaper than the vault-wide `resolve_dangling_links`,
    // which the full `reindex` pass still uses.
    repo.resolve_links_for_note(id).await?;
    Ok(id)
}

/// Apply a set of changed vault-relative paths incrementally: index each path
/// that currently exists on disk, delete each that no longer does. Used by the
/// live watcher so a single note save touches only that note, not the whole
/// vault. Returns the outcome counters (`resolved` is left 0 — `index_file`
/// resolves each indexed note's links inline).
///
/// Classifying by on-disk existence (rather than trusting the `notify` event
/// kind) handles editor atomic-saves uniformly: a save that surfaces as
/// remove-then-create, or as a bare rename-to, still ends with the file present,
/// so it indexes; a real delete ends with it absent, so it prunes.
pub async fn sync_paths(
    vault: &Vault,
    repo: &KnowledgeGraphRepository,
    rels: &[String],
) -> Result<SyncStats> {
    let mut stats = SyncStats::default();
    let mut doomed: Vec<String> = Vec::new();
    for rel in rels {
        if vault.exists(rel) {
            match index_file(vault, repo, rel).await {
                Ok(_) => stats.indexed += 1,
                Err(e) => tracing::warn!("KG sync: failed to index {rel}: {e}"),
            }
        } else {
            doomed.push(rel.clone());
        }
    }
    if !doomed.is_empty() {
        stats.pruned = repo.prune_paths(&doomed).await?;
    }
    Ok(stats)
}

/// Map a parsed note + file metadata into repository inputs.
fn build_inputs(
    rel: &str,
    content: &str,
    checksum: &str,
    meta: Option<&std::fs::Metadata>,
) -> (NoteUpsert, Vec<ObservationInput>, Vec<RelationInput>) {
    let parsed = parser::parse(content);

    let title = parsed
        .title
        .clone()
        .or_else(|| filename_stem(rel))
        .unwrap_or_else(|| rel.to_string());
    let note_type = parsed
        .frontmatter
        .note_type
        .clone()
        .or_else(|| vault::type_from_path(rel));
    let frontmatter_json = parsed.frontmatter.to_json();

    let (mtime, size) = match meta {
        Some(m) => (mtime_secs(m), m.len() as i64),
        None => (0, content.len() as i64),
    };

    let note = NoteUpsert {
        path: rel.to_string(),
        title,
        note_type,
        frontmatter_json,
        body: content.to_string(),
        checksum: checksum.to_string(),
        mtime,
        size,
    };

    let observations = parsed
        .observations
        .iter()
        .map(|o| ObservationInput {
            category: o.category.clone(),
            content: o.content.clone(),
            tags_json: if o.tags.is_empty() {
                None
            } else {
                serde_json::to_string(&o.tags).ok()
            },
            context: o.context.clone(),
        })
        .collect();

    let relations = parsed
        .relations
        .iter()
        .map(|r| RelationInput {
            to_name: r.target.clone(),
            relation_type: r.relation_type.clone(),
            context: r.context.clone(),
        })
        .collect();

    (note, observations, relations)
}

fn checksum_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn mtime_secs(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Resolve the vault, run an initial reindex, then start a live watcher.
/// Spawned at startup when the knowledge-graph tools are enabled. When
/// `git_enabled` is set, the vault is initialized as a git repo first (idempotent)
/// so versioning / the review gate have a repo to operate on.
pub fn spawn_indexer(config: &Config, pool: Pool) {
    let vault = Vault::from_config(config);
    let git_enabled = config.kg_git_active();
    let repo = KnowledgeGraphRepository::new(pool);
    tokio::spawn(async move {
        if git_enabled {
            match super::review::ensure_repo(&vault) {
                Ok(_) => tracing::info!("KG vault git backing ready ({:?})", vault.root()),
                Err(e) => tracing::warn!("KG vault git init failed: {e}"),
            }
        }
        match reindex(&vault, &repo).await {
            Ok(stats) => tracing::info!(
                "KG vault indexed: {} new, {} unchanged, {} pruned, {} links resolved ({:?})",
                stats.indexed,
                stats.skipped,
                stats.pruned,
                stats.resolved,
                vault.root(),
            ),
            Err(e) => tracing::warn!("KG vault initial reindex failed: {e}"),
        }
        spawn_watcher(vault, repo);
    });
}

/// Spawn a `notify` watcher that reindexes the vault on file changes (debounced).
/// Mirrors the config-watcher pattern: a blocking thread owns the watcher and
/// feeds a std mpsc channel; debounced bursts trigger an async reindex on the
/// tokio runtime.
pub fn spawn_watcher(vault: Vault, repo: KnowledgeGraphRepository) -> tokio::task::JoinHandle<()> {
    use notify::{RecursiveMode, Watcher};

    tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        let root = vault.root().to_path_buf();

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(move |res| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("KG watcher: failed to create watcher: {e}");
                return;
            }
        };

        if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
            tracing::warn!("KG watcher: cannot watch {:?}: {e}", root);
            return;
        }
        tracing::info!("KG watcher: watching {:?}", root);

        let debounce = Duration::from_millis(500);
        while let Ok(first) = rx.recv() {
            // Snapshot the suppression generation at the start of the burst. A
            // gated git op (merge/reset/…) bumps this on completion, so any burst
            // it caused — whose events arrive before or during this window — is
            // discarded below rather than racing the authoritative reindex.
            let gen_at_start = suppress_gen();

            // Drain the debounce window so a burst of save events → one pass,
            // accumulating every changed path across the whole burst.
            let mut events = vec![first];
            let deadline = std::time::Instant::now() + debounce;
            loop {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    break;
                }
                match rx.recv_timeout(remaining) {
                    Ok(ev) => events.push(ev),
                    Err(_) => break,
                }
            }

            // Drop the burst if a gated op is in flight or completed during the
            // window — its own explicit reindex is the sole index update.
            if suppressed() || suppress_gen() != gen_at_start {
                continue;
            }

            // Classify the burst: collect the distinct `.md` notes touched, and
            // note whether any folder-scope change demands a full reindex.
            let mut notes: Vec<String> = Vec::new();
            let mut needs_full = false;
            for event in &events {
                for path in &event.paths {
                    match vault.classify_path(path) {
                        PathClass::Note(rel) => {
                            if !notes.contains(&rel) {
                                notes.push(rel);
                            }
                        }
                        PathClass::Other => needs_full = true,
                        PathClass::Ignore => {}
                    }
                }
            }

            if !needs_full && notes.is_empty() {
                continue; // Pure `.obsidian/` noise — nothing to do.
            }

            let vault = vault.clone();
            let repo = repo.clone();
            rt.spawn(async move {
                // A folder-scope change (dir rename/delete) can move child notes
                // with no per-note event, so fall back to a full reindex; that
                // pass also re-syncs the individually-touched notes.
                let result = if needs_full {
                    reindex(&vault, &repo).await
                } else {
                    sync_paths(&vault, &repo, &notes).await
                };
                match result {
                    Ok(stats) if stats.indexed > 0 || stats.pruned > 0 => tracing::debug!(
                        "KG watcher sync: {} indexed, {} pruned, {} resolved",
                        stats.indexed,
                        stats.pruned,
                        stats.resolved
                    ),
                    Ok(_) => {}
                    Err(e) => tracing::warn!("KG watcher sync failed: {e}"),
                }
            });
        }
        tracing::info!("KG watcher: stopped");
    })
}
