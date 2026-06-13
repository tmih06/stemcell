//! Orchestration for the git-backed knowledge-graph review gate.
//!
//! This is the single service layer the `kg_remember` tool and the `/kg` TUI
//! both call into, so the policy (git ops + queue rows + watcher suppression +
//! reindex) lives in one place rather than being duplicated across callers.
//!
//! ## The staging model
//!
//! A queued batch is committed onto a `kg/batch/<id>` branch checked out in a
//! **sibling** worktree at `<vault>/../<STAGING_DIR>/<id>` — deliberately outside
//! the watched vault root, so writing the batch trips no `notify` events. Only
//! the final merge/revert/restore mutates the watched tree, and each of those
//! runs under a [`sync::suppress_begin`] guard followed by one authoritative
//! [`sync::reindex`], so the watcher never races a half-applied git operation.
//!
//! The vault DB index always reflects **main** (approved) content. A pending
//! batch's writes live only on its branch until approved — they are intentionally
//! invisible to `kg_search`/`kg_read` until the user accepts them.

use super::compose;
use super::git_review::{GitRepo, LogEntry, MergeOutcome};
use super::sync;
use super::vault::Vault;
use crate::config::Config;
use crate::db::{KgBatchStats, KgPendingBatchRepository, KnowledgeGraphRepository, Pool};
use anyhow::{Context, Result, bail};
use serde_json::Value;

/// Sibling directory (next to the vault root) that holds per-batch worktrees.
const STAGING_DIR: &str = ".kg-staging";

/// One note's worth of write input for [`queue_batch`].
#[derive(Debug, Clone)]
pub struct NoteInput {
    pub title: String,
    pub note_type: Option<String>,
    pub observations: Vec<String>,
    pub relations: Vec<Value>,
}

/// Result of queuing a batch.
#[derive(Debug, Clone)]
pub struct QueuedBatch {
    pub id: String,
    pub notes_written: usize,
    pub files_changed: i64,
}

/// Resolve the staging worktree path for a batch id (sibling of the vault root).
fn staging_path(vault: &Vault, id: &str) -> std::path::PathBuf {
    let root = vault.root();
    let parent = root.parent().unwrap_or(root);
    parent.join(STAGING_DIR).join(id)
}

/// Ensure the vault is a git repo, scaffolding folders + git metadata. Idempotent.
/// Called at startup (when `kg_git_enabled`) and lazily before the first write.
pub fn ensure_repo(vault: &Vault) -> Result<GitRepo> {
    vault
        .ensure_scaffold()
        .with_context(|| format!("failed to scaffold vault at {:?}", vault.root()))?;
    let repo = GitRepo::open(vault.root());
    repo.ensure_repo()
        .with_context(|| format!("failed to init git in vault at {:?}", vault.root()))?;
    Ok(repo)
}

/// Compose the markdown for one note against the current on-branch state. A new
/// note is built fresh; an existing one gets its bullets surgically appended.
/// Returns `(relative_path, content)`.
async fn compose_note(
    kg_repo: &KnowledgeGraphRepository,
    stage_vault: &Vault,
    note: &NoteInput,
) -> Result<(String, String)> {
    let observation_bullets: Vec<String> = note
        .observations
        .iter()
        .map(|s| compose::observation_bullet(s))
        .collect();
    let relation_bullets: Vec<String> = note
        .relations
        .iter()
        .filter_map(compose::relation_bullet)
        .collect();

    let rel = compose::resolve_note_rel(kg_repo, &note.title, note.note_type.as_deref()).await?;

    let existing = stage_vault
        .exists(&rel)
        .then(|| stage_vault.read_note(&rel).unwrap_or_default());
    let (content, _, _) = compose::compose_content(
        existing.as_deref(),
        &note.title,
        note.note_type.as_deref(),
        &observation_bullets,
        &relation_bullets,
    );
    Ok((rel, content))
}

/// Queue a batch of note writes onto a fresh `kg/batch/<id>` branch in a sibling
/// worktree, commit them, and park a `pending` row. Writes happen outside the
/// watched root, so no watcher suppression is needed here.
pub async fn queue_batch(
    config: &Config,
    pool: Pool,
    summary: &str,
    notes: &[NoteInput],
) -> Result<QueuedBatch> {
    if notes.is_empty() {
        bail!("a batch needs at least one note");
    }
    let vault = Vault::from_config(config);
    let repo = ensure_repo(&vault)?;
    let kg_repo = KnowledgeGraphRepository::new(pool.clone());
    let queue = KgPendingBatchRepository::new(pool);

    let base_sha = repo.head_sha()?;
    let id = uuid::Uuid::new_v4().to_string();
    let branch = format!("kg/batch/{id}");
    let stage = staging_path(&vault, &id);

    repo.create_batch_worktree(&branch, &stage)
        .with_context(|| format!("failed to create staging worktree for batch {id}"))?;

    // Compose + write each note against the staging worktree's own view, so
    // multiple notes in one batch that touch the same file accumulate correctly.
    let stage_vault = Vault::open(&stage);
    let mut written = 0usize;
    for note in notes {
        let (rel, content) = match compose_note(&kg_repo, &stage_vault, note).await {
            Ok(pair) => pair,
            Err(e) => {
                // Roll back the half-built worktree so a bad note doesn't leave
                // an orphan branch behind.
                let _ = repo.remove_worktree(&stage);
                let _ = repo.delete_branch(&branch);
                return Err(e).with_context(|| format!("failed to compose note {:?}", note.title));
            }
        };
        if let Err(e) = stage_vault.write_note(&rel, &content) {
            let _ = repo.remove_worktree(&stage);
            let _ = repo.delete_branch(&branch);
            return Err(anyhow::anyhow!(e))
                .with_context(|| format!("failed to write {rel} in staging worktree"));
        }
        written += 1;
    }

    repo.commit_worktree(&stage, summary)
        .with_context(|| format!("failed to commit batch {id}"))?;

    let stat = repo.diff_stat(&base_sha, &branch).unwrap_or_default();
    queue
        .insert(
            &id,
            &branch,
            &base_sha,
            summary,
            &stage.to_string_lossy(),
            KgBatchStats {
                files_changed: stat.files_changed,
                insertions: stat.insertions,
                deletions: stat.deletions,
            },
        )
        .await?;

    Ok(QueuedBatch {
        id,
        notes_written: written,
        files_changed: stat.files_changed,
    })
}

/// Approve a pending batch: merge its branch into main, reindex, mark approved,
/// and clean up the worktree + branch. On a true conflict the batch is marked
/// `conflicted` (branch + worktree kept for manual resolution) and the index is
/// left untouched.
pub async fn approve(config: &Config, pool: Pool, batch_id: &str) -> Result<MergeOutcome> {
    let vault = Vault::from_config(config);
    let repo = GitRepo::open(vault.root());
    let kg_repo = KnowledgeGraphRepository::new(pool.clone());
    let queue = KgPendingBatchRepository::new(pool);

    let batch = queue
        .get(batch_id)
        .await?
        .with_context(|| format!("no such batch {batch_id}"))?;
    if batch.status != "pending" && batch.status != "conflicted" {
        bail!(
            "batch {batch_id} is {} — only pending can be approved",
            batch.status
        );
    }

    let merge_msg = format!("kg: approve {}", batch.summary);
    // Suppress the watcher across the merge, then do one authoritative reindex.
    let outcome = {
        let _guard = sync::suppress_begin();
        repo.merge_batch(&batch.branch, &merge_msg)?
    };

    match &outcome {
        MergeOutcome::Merged(merge_sha) => {
            sync::reindex(&vault, &kg_repo).await?;
            queue.mark_approved(batch_id, merge_sha).await?;
            if let Some(wt) = &batch.worktree_path {
                let _ = repo.remove_worktree(std::path::Path::new(wt));
            }
            let _ = repo.delete_branch(&batch.branch);
        }
        MergeOutcome::Conflicted(_) => {
            queue.mark_conflicted(batch_id).await?;
        }
    }
    Ok(outcome)
}

/// Decline a pending batch: drop its branch + worktree and mark it declined.
/// Main is untouched, so no reindex is needed.
pub async fn decline(config: &Config, pool: Pool, batch_id: &str) -> Result<()> {
    let vault = Vault::from_config(config);
    let repo = GitRepo::open(vault.root());
    let queue = KgPendingBatchRepository::new(pool);

    let batch = queue
        .get(batch_id)
        .await?
        .with_context(|| format!("no such batch {batch_id}"))?;
    if let Some(wt) = &batch.worktree_path {
        let _ = repo.remove_worktree(std::path::Path::new(wt));
    }
    let _ = repo.delete_branch(&batch.branch);
    queue.mark_declined(batch_id).await?;
    Ok(())
}

/// Revert the most-recently-approved batch's merge commit, then reindex.
pub async fn revert_last(config: &Config, pool: Pool) -> Result<String> {
    let vault = Vault::from_config(config);
    let repo = GitRepo::open(vault.root());
    let kg_repo = KnowledgeGraphRepository::new(pool.clone());
    let queue = KgPendingBatchRepository::new(pool);

    let batch = queue
        .last_approved()
        .await?
        .context("no approved batch to revert")?;
    let merge_sha = batch
        .merge_sha
        .context("approved batch is missing its merge sha")?;

    let new_head = {
        let _guard = sync::suppress_begin();
        repo.revert_merge(&merge_sha)?
    };
    sync::reindex(&vault, &kg_repo).await?;
    Ok(new_head)
}

/// Hard-reset the vault to an arbitrary historical commit, then reindex.
/// Destructive — the caller (TUI) confirms first.
pub async fn restore(config: &Config, pool: Pool, sha: &str) -> Result<()> {
    let vault = Vault::from_config(config);
    let repo = GitRepo::open(vault.root());
    let kg_repo = KnowledgeGraphRepository::new(pool);

    {
        let _guard = sync::suppress_begin();
        repo.reset_hard(sha)?;
    }
    sync::reindex(&vault, &kg_repo).await?;
    Ok(())
}

/// Pending (and conflicted) batches awaiting review, newest first. A conflicted
/// batch is still actionable — the user can decline it or resolve manually — so
/// both states surface in the `/kg` queue.
pub async fn list_pending(pool: Pool) -> Result<Vec<crate::db::KgPendingBatch>> {
    KgPendingBatchRepository::new(pool)
        .list_by_statuses(&["pending", "conflicted"])
        .await
}

/// The full patch for a pending batch (its branch vs. its recorded base).
pub async fn batch_diff(config: &Config, pool: Pool, batch_id: &str) -> Result<String> {
    let vault = Vault::from_config(config);
    let repo = GitRepo::open(vault.root());
    let queue = KgPendingBatchRepository::new(pool);
    let batch = queue
        .get(batch_id)
        .await?
        .with_context(|| format!("no such batch {batch_id}"))?;
    repo.diff_patch(&batch.base_sha, &batch.branch)
}

/// Recent vault history, newest first.
pub fn log(config: &Config, limit: usize) -> Result<Vec<LogEntry>> {
    let vault = Vault::from_config(config);
    GitRepo::open(vault.root()).log(limit)
}

/// Full `git show` for a commit.
pub fn show(config: &Config, sha: &str) -> Result<String> {
    let vault = Vault::from_config(config);
    GitRepo::open(vault.root()).show(sha)
}
