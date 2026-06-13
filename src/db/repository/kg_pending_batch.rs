//! Knowledge-graph review-queue repository.
//!
//! One row per parked memory-write batch (`kg_pending_batch`, migration
//! `20260613000001_add_kg_pending_batch.sql`). When the review gate is on, the
//! `kg_remember` tool seals its writes onto a git branch in a sibling worktree
//! and inserts a `pending` row here; the user approves (→ `approved`, carrying
//! the merge sha for later revert), declines (→ `declined`), or the merge hits a
//! true conflict (→ `conflicted`). The diff is never stored — it is reproducible
//! from `branch` against `base_sha` — so this table holds only batch state plus
//! cached `git diff --shortstat` counts for the `/kg` list view.

use crate::db::Pool;
use crate::db::database::interact_err;
use anyhow::{Context, Result};
use rusqlite::params;

/// A parked memory-write batch awaiting (or past) review.
#[derive(Debug, Clone)]
pub struct KgPendingBatch {
    pub id: String,
    pub branch: String,
    pub base_sha: String,
    pub summary: String,
    pub status: String,
    pub worktree_path: Option<String>,
    pub merge_sha: Option<String>,
    pub files_changed: i64,
    pub insertions: i64,
    pub deletions: i64,
    pub created_at: i64,
}

/// Cached diff counts for a batch (from `git diff --shortstat`).
#[derive(Debug, Clone, Default)]
pub struct KgBatchStats {
    pub files_changed: i64,
    pub insertions: i64,
    pub deletions: i64,
}

#[derive(Clone)]
pub struct KgPendingBatchRepository {
    pool: Pool,
}

impl KgPendingBatchRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Insert a freshly-queued batch (`pending`).
    pub async fn insert(
        &self,
        id: &str,
        branch: &str,
        base_sha: &str,
        summary: &str,
        worktree_path: &str,
        stats: KgBatchStats,
    ) -> Result<()> {
        let (id, branch, base_sha, summary, worktree_path) = (
            id.to_string(),
            branch.to_string(),
            base_sha.to_string(),
            summary.to_string(),
            worktree_path.to_string(),
        );
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| {
                conn.execute(
                    "INSERT INTO kg_pending_batch \
                       (id, branch, base_sha, summary, status, worktree_path, \
                        files_changed, insertions, deletions) \
                     VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?7, ?8)",
                    params![
                        id,
                        branch,
                        base_sha,
                        summary,
                        worktree_path,
                        stats.files_changed,
                        stats.insertions,
                        stats.deletions,
                    ],
                )
            })
            .await
            .map_err(interact_err)?
            .context("Failed to insert pending batch")?;
        Ok(())
    }

    /// All batches with a given status, newest first.
    pub async fn list_by_status(&self, status: &str) -> Result<Vec<KgPendingBatch>> {
        let status = status.to_string();
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, branch, base_sha, summary, status, worktree_path, merge_sha, \
                            files_changed, insertions, deletions, created_at \
                     FROM kg_pending_batch WHERE status = ?1 \
                     ORDER BY created_at DESC, rowid DESC",
                )?;
                let rows = stmt.query_map(params![status], batch_from_row)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })
            .await
            .map_err(interact_err)?
            .context("Failed to list pending batches")
    }

    /// All batches in any of `statuses`, newest first. One query for the
    /// review queue (`pending` + `conflicted`) instead of a round-trip per status.
    pub async fn list_by_statuses(&self, statuses: &[&str]) -> Result<Vec<KgPendingBatch>> {
        if statuses.is_empty() {
            return Ok(Vec::new());
        }
        let statuses: Vec<String> = statuses.iter().map(|s| s.to_string()).collect();
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| {
                let placeholders = vec!["?"; statuses.len()].join(", ");
                let sql = format!(
                    "SELECT id, branch, base_sha, summary, status, worktree_path, merge_sha, \
                            files_changed, insertions, deletions, created_at \
                     FROM kg_pending_batch WHERE status IN ({placeholders}) \
                     ORDER BY created_at DESC, rowid DESC"
                );
                let mut stmt = conn.prepare(&sql)?;
                let params = rusqlite::params_from_iter(statuses.iter());
                let rows = stmt.query_map(params, batch_from_row)?;
                rows.collect::<rusqlite::Result<Vec<_>>>()
            })
            .await
            .map_err(interact_err)?
            .context("Failed to list batches by status")
    }

    /// Fetch one batch by id.
    pub async fn get(&self, id: &str) -> Result<Option<KgPendingBatch>> {
        let id = id.to_string();
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| {
                let mut stmt = conn.prepare_cached(
                    "SELECT id, branch, base_sha, summary, status, worktree_path, merge_sha, \
                            files_changed, insertions, deletions, created_at \
                     FROM kg_pending_batch WHERE id = ?1",
                )?;
                stmt.query_row(params![id], batch_from_row)
                    .map(Some)
                    .or_else(|e| match e {
                        rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        other => Err(other),
                    })
            })
            .await
            .map_err(interact_err)?
            .context("Failed to get pending batch")
    }

    /// Mark a batch approved, recording the merge commit sha (for later revert)
    /// and clearing the worktree path (the worktree is removed on approve).
    pub async fn mark_approved(&self, id: &str, merge_sha: &str) -> Result<()> {
        let (id, merge_sha) = (id.to_string(), merge_sha.to_string());
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| {
                conn.execute(
                    "UPDATE kg_pending_batch \
                     SET status = 'approved', merge_sha = ?2, worktree_path = NULL, \
                         resolved_at = strftime('%s','now') \
                     WHERE id = ?1",
                    params![id, merge_sha],
                )
            })
            .await
            .map_err(interact_err)?
            .context("Failed to mark batch approved")?;
        Ok(())
    }

    /// Mark a batch declined and clear its (now-removed) worktree path.
    pub async fn mark_declined(&self, id: &str) -> Result<()> {
        self.set_terminal_status(id, "declined").await
    }

    /// Mark a batch conflicted — the merge couldn't auto-resolve. The branch and
    /// worktree are kept so the user can resolve manually.
    pub async fn mark_conflicted(&self, id: &str) -> Result<()> {
        let id = id.to_string();
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| {
                conn.execute(
                    "UPDATE kg_pending_batch \
                     SET status = 'conflicted', resolved_at = strftime('%s','now') \
                     WHERE id = ?1",
                    params![id],
                )
            })
            .await
            .map_err(interact_err)?
            .context("Failed to mark batch conflicted")?;
        Ok(())
    }

    async fn set_terminal_status(&self, id: &str, status: &str) -> Result<()> {
        let (id, status) = (id.to_string(), status.to_string());
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| {
                conn.execute(
                    "UPDATE kg_pending_batch \
                     SET status = ?2, worktree_path = NULL, resolved_at = strftime('%s','now') \
                     WHERE id = ?1",
                    params![id, status],
                )
            })
            .await
            .map_err(interact_err)?
            .context("Failed to update batch status")?;
        Ok(())
    }

    /// The most recently approved batch (the revert target for `/kg revert`).
    pub async fn last_approved(&self) -> Result<Option<KgPendingBatch>> {
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, branch, base_sha, summary, status, worktree_path, merge_sha, \
                            files_changed, insertions, deletions, created_at \
                     FROM kg_pending_batch \
                     WHERE status = 'approved' AND merge_sha IS NOT NULL \
                     ORDER BY resolved_at DESC, rowid DESC LIMIT 1",
                )?;
                stmt.query_row([], batch_from_row)
                    .map(Some)
                    .or_else(|e| match e {
                        rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        other => Err(other),
                    })
            })
            .await
            .map_err(interact_err)?
            .context("Failed to get last approved batch")
    }
}

fn batch_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KgPendingBatch> {
    Ok(KgPendingBatch {
        id: row.get(0)?,
        branch: row.get(1)?,
        base_sha: row.get(2)?,
        summary: row.get(3)?,
        status: row.get(4)?,
        worktree_path: row.get(5)?,
        merge_sha: row.get(6)?,
        files_changed: row.get(7)?,
        insertions: row.get(8)?,
        deletions: row.get(9)?,
        created_at: row.get(10)?,
    })
}
