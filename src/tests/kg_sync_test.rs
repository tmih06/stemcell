//! Tests for the filesystem → SQLite vault indexer (`brain::kg::sync`).
//!
//! Each test uses a `tempfile` vault + an in-memory database, exercising the
//! full read → parse → index → prune → resolve pipeline against real files.

use crate::brain::kg::sync::{self, reindex};
use crate::brain::kg::vault::Vault;
use crate::db::repository::LinkDirection;
use crate::db::{Database, KnowledgeGraphRepository};

async fn setup() -> (tempfile::TempDir, Database, Vault, KnowledgeGraphRepository) {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = Vault::open(dir.path());
    let db = Database::connect_in_memory().await.expect("db");
    db.run_migrations().await.expect("migrations");
    let repo = KnowledgeGraphRepository::new(db.pool().clone());
    (dir, db, vault, repo)
}

#[tokio::test]
async fn reindex_indexes_notes_and_scaffolds() {
    let (_dir, _db, vault, repo) = setup().await;
    vault
        .write_note(
            "concepts/Rust Async.md",
            "---\ntitle: Rust Async\n---\n\n# Rust Async\n\nFutures are lazy.\n",
        )
        .expect("write");
    vault
        .write_note("people/Alice.md", "# Alice\n\nWorks on async.\n")
        .expect("write");

    let stats = reindex(&vault, &repo).await.expect("reindex");
    assert_eq!(stats.indexed, 2);
    assert_eq!(repo.note_count().await.expect("count"), 2);

    // Scaffold created the marker + folders.
    assert!(vault.root().join(".obsidian").exists());
    assert!(vault.root().join("concepts").exists());

    // Note type inferred from the folder when frontmatter omits it.
    let alice = repo
        .get_note_by_path("people/Alice.md")
        .await
        .expect("q")
        .expect("present");
    assert_eq!(alice.note_type.as_deref(), Some("person"));
}

#[tokio::test]
async fn unchanged_files_are_skipped_on_second_pass() {
    let (_dir, _db, vault, repo) = setup().await;
    vault.write_note("a.md", "# A\nbody\n").expect("write");
    vault.write_note("b.md", "# B\nbody\n").expect("write");

    let first = reindex(&vault, &repo).await.expect("first");
    assert_eq!(first.indexed, 2);
    assert_eq!(first.skipped, 0);

    let second = reindex(&vault, &repo).await.expect("second");
    assert_eq!(second.indexed, 0, "checksums unchanged → nothing reindexed");
    assert_eq!(second.skipped, 2);
}

#[tokio::test]
async fn changed_file_is_reindexed() {
    let (_dir, _db, vault, repo) = setup().await;
    vault.write_note("a.md", "# A\noriginal\n").expect("write");
    vault.write_note("b.md", "# B\nbody\n").expect("write");
    reindex(&vault, &repo).await.expect("first");

    vault
        .write_note("a.md", "# A\nedited content\n")
        .expect("rewrite");
    let stats = reindex(&vault, &repo).await.expect("second");
    assert_eq!(stats.indexed, 1, "only the edited file reindexes");
    assert_eq!(stats.skipped, 1);

    let hits = repo.search_fts("edited", 5).await.expect("search");
    assert_eq!(hits.len(), 1);
}

#[tokio::test]
async fn deleted_file_is_pruned() {
    let (_dir, _db, vault, repo) = setup().await;
    vault.write_note("a.md", "# A\nbody\n").expect("write");
    vault.write_note("b.md", "# B\nbody\n").expect("write");
    reindex(&vault, &repo).await.expect("first");
    assert_eq!(repo.note_count().await.expect("count"), 2);

    std::fs::remove_file(vault.note_path("b.md")).expect("rm");
    let stats = reindex(&vault, &repo).await.expect("second");
    assert_eq!(stats.pruned, 1);
    assert_eq!(repo.note_count().await.expect("count"), 1);
    assert!(repo.get_note_by_path("b.md").await.expect("q").is_none());
}

#[tokio::test]
async fn dangling_links_back_fill_after_reindex() {
    let (_dir, _db, vault, repo) = setup().await;
    vault
        .write_note(
            "concepts/Rust Async.md",
            "# Rust Async\n\n## Relations\n- depends_on [[Tokio Runtime]]\n",
        )
        .expect("write");
    vault
        .write_note(
            "concepts/Tokio Runtime.md",
            "# Tokio Runtime\n\nThe runtime.\n",
        )
        .expect("write");

    let stats = reindex(&vault, &repo).await.expect("reindex");
    assert!(stats.resolved >= 1, "the depends_on link should resolve");

    let a = repo
        .get_note_by_path("concepts/Rust Async.md")
        .await
        .expect("q")
        .expect("present");
    let b = repo
        .get_note_by_path("concepts/Tokio Runtime.md")
        .await
        .expect("q")
        .expect("present");

    let out = repo.neighbors(a.id, LinkDirection::Out).await.expect("out");
    let dep = out
        .iter()
        .find(|n| n.relation_type == "depends_on")
        .expect("depends_on edge");
    assert_eq!(dep.other_id, Some(b.id), "link resolved to the target note");
}

#[tokio::test]
async fn index_file_indexes_single_note() {
    let (_dir, _db, vault, repo) = setup().await;
    vault.ensure_scaffold().expect("scaffold");
    vault
        .write_note("concepts/New.md", "# New Note\n\nFresh content here.\n")
        .expect("write");

    let id = sync::index_file(&vault, &repo, "concepts/New.md")
        .await
        .expect("index_file");
    assert!(id > 0);
    let got = repo
        .get_note_by_path("concepts/New.md")
        .await
        .expect("q")
        .expect("present");
    assert_eq!(got.title, "New Note");
}
