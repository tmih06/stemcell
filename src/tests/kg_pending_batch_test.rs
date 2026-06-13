//! Tests for the knowledge-graph review-queue repository
//! (`db::repository::kg_pending_batch`). In-memory DB, no git involved.

use crate::db::Database;
use crate::db::repository::{KgBatchStats, KgPendingBatchRepository};

async fn setup() -> (Database, KgPendingBatchRepository) {
    let db = Database::connect_in_memory().await.expect("db");
    db.run_migrations().await.expect("migrations");
    let repo = KgPendingBatchRepository::new(db.pool().clone());
    (db, repo)
}

fn stats(files: i64, ins: i64, del: i64) -> KgBatchStats {
    KgBatchStats {
        files_changed: files,
        insertions: ins,
        deletions: del,
    }
}

#[tokio::test]
async fn insert_and_get_round_trips() {
    let (_db, repo) = setup().await;
    repo.insert(
        "b1",
        "kg/batch/b1",
        "deadbeef",
        "remember rust async",
        "/tmp/stage/b1",
        stats(2, 7, 1),
    )
    .await
    .expect("insert");

    let got = repo.get("b1").await.expect("get").expect("present");
    assert_eq!(got.branch, "kg/batch/b1");
    assert_eq!(got.base_sha, "deadbeef");
    assert_eq!(got.status, "pending");
    assert_eq!(got.worktree_path.as_deref(), Some("/tmp/stage/b1"));
    assert_eq!(got.files_changed, 2);
    assert_eq!(got.insertions, 7);
    assert!(got.merge_sha.is_none());
}

#[tokio::test]
async fn list_by_status_filters_and_orders_newest_first() {
    let (_db, repo) = setup().await;
    for id in ["a", "b", "c"] {
        repo.insert(
            id,
            &format!("kg/batch/{id}"),
            "base",
            "s",
            "/wt",
            stats(1, 1, 0),
        )
        .await
        .expect("insert");
    }
    repo.mark_declined("b").await.expect("decline");

    let pending = repo.list_by_status("pending").await.expect("list");
    let ids: Vec<&str> = pending.iter().map(|b| b.id.as_str()).collect();
    assert_eq!(ids, vec!["c", "a"], "newest-first, declined 'b' excluded");

    let declined = repo.list_by_status("declined").await.expect("list");
    assert_eq!(declined.len(), 1);
    assert_eq!(declined[0].id, "b");
}

#[tokio::test]
async fn approve_records_merge_sha_and_clears_worktree() {
    let (_db, repo) = setup().await;
    repo.insert("m1", "kg/batch/m1", "base", "s", "/wt", stats(1, 3, 0))
        .await
        .expect("insert");
    repo.mark_approved("m1", "merge123").await.expect("approve");

    let got = repo.get("m1").await.expect("get").expect("present");
    assert_eq!(got.status, "approved");
    assert_eq!(got.merge_sha.as_deref(), Some("merge123"));
    assert!(got.worktree_path.is_none(), "worktree cleared on approve");
}

#[tokio::test]
async fn last_approved_returns_most_recent_revert_target() {
    let (_db, repo) = setup().await;
    repo.insert("x", "kg/batch/x", "base", "s", "/wt", stats(1, 1, 0))
        .await
        .expect("x");
    repo.insert("y", "kg/batch/y", "base", "s", "/wt", stats(1, 1, 0))
        .await
        .expect("y");
    repo.mark_approved("x", "mx").await.expect("ax");
    repo.mark_approved("y", "my").await.expect("ay");

    let last = repo.last_approved().await.expect("last").expect("present");
    assert_eq!(last.id, "y", "most recently approved is the revert target");
    assert_eq!(last.merge_sha.as_deref(), Some("my"));
}

#[tokio::test]
async fn conflicted_keeps_worktree_for_manual_resolution() {
    let (_db, repo) = setup().await;
    repo.insert("c1", "kg/batch/c1", "base", "s", "/wt/c1", stats(1, 2, 2))
        .await
        .expect("insert");
    repo.mark_conflicted("c1").await.expect("conflict");

    let got = repo.get("c1").await.expect("get").expect("present");
    assert_eq!(got.status, "conflicted");
    assert_eq!(
        got.worktree_path.as_deref(),
        Some("/wt/c1"),
        "conflicted batch keeps its worktree so the user can resolve"
    );
}
