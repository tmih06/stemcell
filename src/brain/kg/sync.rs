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
use super::vault::{self, Vault};
use crate::config::Config;
use crate::db::{KnowledgeGraphRepository, NoteUpsert, ObservationInput, Pool, RelationInput};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::time::{Duration, UNIX_EPOCH};

/// Outcome counters for a sync pass.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncStats {
    pub indexed: usize,
    pub skipped: usize,
    pub pruned: usize,
    pub resolved: usize,
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
    let mut seen: Vec<String> = Vec::new();

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
        seen.push(rel.clone());

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

    stats.pruned = repo.prune_missing(&seen).await?;
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
/// Spawned at startup when the knowledge-graph tools are enabled.
pub fn spawn_indexer(config: &Config, pool: Pool) {
    let vault = Vault::from_config(config);
    let repo = KnowledgeGraphRepository::new(pool);
    tokio::spawn(async move {
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
        while rx.recv().is_ok() {
            // Drain the debounce window so a burst of save events → one reindex.
            let deadline = std::time::Instant::now() + debounce;
            loop {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() || rx.recv_timeout(remaining).is_err() {
                    break;
                }
            }

            let vault = vault.clone();
            let repo = repo.clone();
            rt.spawn(async move {
                match reindex(&vault, &repo).await {
                    Ok(stats) if stats.indexed > 0 || stats.pruned > 0 => tracing::debug!(
                        "KG watcher reindex: {} indexed, {} pruned, {} resolved",
                        stats.indexed,
                        stats.pruned,
                        stats.resolved
                    ),
                    Ok(_) => {}
                    Err(e) => tracing::warn!("KG watcher reindex failed: {e}"),
                }
            });
        }
        tracing::info!("KG watcher: stopped");
    })
}
