//! Tests for the filesystem → SQLite vault indexer (`brain::kg::sync`).
//!
//! Each test uses a `tempfile` vault + an in-memory database, exercising the
//! full read → parse → index → prune → resolve pipeline against real files.

use crate::brain::kg::sync::{self, reindex, sync_paths};
use crate::brain::kg::vault::{PathClass, Vault};
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

#[tokio::test]
async fn index_file_resolves_links_both_directions() {
    let (_dir, _db, vault, repo) = setup().await;
    vault.ensure_scaffold().expect("scaffold");

    // A links forward to a not-yet-indexed B; indexing A leaves a ghost.
    vault
        .write_note(
            "concepts/Rust Async.md",
            "# Rust Async\n\n## Relations\n- depends_on [[Tokio Runtime]]\n",
        )
        .expect("write a");
    let a = sync::index_file(&vault, &repo, "concepts/Rust Async.md")
        .await
        .expect("index a");
    let a_out = repo.neighbors(a, LinkDirection::Out).await.expect("a out");
    assert!(a_out[0].other_id.is_none(), "ghost until B is indexed");

    // B links back to A. Indexing B via the scoped path must resolve both
    // B→A (outgoing) and the pre-existing A→B ghost (incoming back-fill).
    vault
        .write_note(
            "concepts/Tokio Runtime.md",
            "# Tokio Runtime\n\n## Relations\n- used_by [[Rust Async]]\n",
        )
        .expect("write b");
    let b = sync::index_file(&vault, &repo, "concepts/Tokio Runtime.md")
        .await
        .expect("index b");

    let a_out2 = repo.neighbors(a, LinkDirection::Out).await.expect("a out2");
    assert_eq!(
        a_out2[0].other_id,
        Some(b),
        "A→B back-filled by scoped resolve"
    );
    let b_out = repo.neighbors(b, LinkDirection::Out).await.expect("b out");
    assert_eq!(b_out[0].other_id, Some(a), "B→A resolved by scoped resolve");
}

#[tokio::test]
async fn sync_paths_indexes_present_and_deletes_absent() {
    let (_dir, _db, vault, repo) = setup().await;
    vault.ensure_scaffold().expect("scaffold");
    vault.write_note("a.md", "# A\nbody\n").expect("write a");
    vault.write_note("b.md", "# B\nbody\n").expect("write b");

    // First sync indexes both present files.
    let stats = sync_paths(&vault, &repo, &["a.md".to_string(), "b.md".to_string()])
        .await
        .expect("sync");
    assert_eq!(stats.indexed, 2);
    assert_eq!(stats.pruned, 0);
    assert_eq!(repo.note_count().await.expect("count"), 2);

    // Delete b.md on disk, then sync only the touched paths: a re-indexes, b prunes.
    std::fs::remove_file(vault.note_path("b.md")).expect("rm");
    let stats = sync_paths(&vault, &repo, &["a.md".to_string(), "b.md".to_string()])
        .await
        .expect("sync2");
    assert_eq!(stats.indexed, 1, "a.md still present → re-indexed");
    assert_eq!(stats.pruned, 1, "b.md gone → pruned");
    assert!(repo.get_note_by_path("b.md").await.expect("q").is_none());
    assert!(repo.get_note_by_path("a.md").await.expect("q").is_some());
}

#[tokio::test]
async fn sync_paths_resolves_links_incrementally() {
    let (_dir, _db, vault, repo) = setup().await;
    vault.ensure_scaffold().expect("scaffold");
    vault
        .write_note(
            "concepts/Rust Async.md",
            "# Rust Async\n\n## Relations\n- depends_on [[Tokio Runtime]]\n",
        )
        .expect("write a");
    vault
        .write_note("concepts/Tokio Runtime.md", "# Tokio Runtime\n\nbody\n")
        .expect("write b");

    // Syncing both paths in one pass must resolve the depends_on edge — the
    // incremental path is not just a full-reindex shortcut.
    sync_paths(
        &vault,
        &repo,
        &[
            "concepts/Rust Async.md".to_string(),
            "concepts/Tokio Runtime.md".to_string(),
        ],
    )
    .await
    .expect("sync");

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
    assert_eq!(dep.other_id, Some(b.id), "link resolved incrementally");
}

#[tokio::test]
async fn classify_path_routes_event_paths() {
    let (_dir, _db, vault, _repo) = setup().await;
    let root = vault.root();

    // A real `.md` note under a folder → indexable, vault-relative path.
    assert_eq!(
        vault.classify_path(&root.join("concepts/Rust Async.md")),
        PathClass::Note("concepts/Rust Async.md".to_string())
    );
    // A top-level note.
    assert_eq!(
        vault.classify_path(&root.join("a.md")),
        PathClass::Note("a.md".to_string())
    );
    // `.obsidian/` write noise → ignored, never triggers indexing.
    assert_eq!(
        vault.classify_path(&root.join(".obsidian/workspace.json")),
        PathClass::Ignore
    );
    // A path outside the vault → ignored.
    assert_eq!(
        vault.classify_path(std::path::Path::new("/etc/passwd")),
        PathClass::Ignore
    );
    // A non-`.md` file or directory under a real folder → folder-scope change.
    assert_eq!(
        vault.classify_path(&root.join("concepts/image.png")),
        PathClass::Other
    );
    assert_eq!(
        vault.classify_path(&root.join("concepts")),
        PathClass::Other
    );
}

#[tokio::test]
async fn reindex_prunes_from_pre_walk_snapshot_only() {
    // Guards the walk/prune TOCTOU fix: prune deletes only paths present in the
    // pre-walk DB snapshot, so a note inserted concurrently (modeled here as one
    // indexed after the prior reindex but absent from disk) is never collateral.
    let (_dir, _db, vault, repo) = setup().await;
    vault.write_note("a.md", "# A\nbody\n").expect("write a");
    reindex(&vault, &repo).await.expect("first");
    assert_eq!(repo.note_count().await.expect("count"), 1);

    // Index a note whose file does not exist on disk, mimicking a row a
    // concurrent kg_note write committed after the walk captured `seen`.
    vault
        .write_note("ghost.md", "# Ghost\nbody\n")
        .expect("write");
    sync::index_file(&vault, &repo, "ghost.md")
        .await
        .expect("index ghost");
    std::fs::remove_file(vault.note_path("ghost.md")).expect("rm");

    // A reindex walk now will not see ghost.md on disk. Because ghost.md WAS in
    // the pre-walk snapshot, it is a legitimate prune target — confirming prune
    // works off the snapshot. a.md (present) survives.
    let stats = reindex(&vault, &repo).await.expect("second");
    assert_eq!(stats.pruned, 1, "ghost.md pruned from snapshot");
    assert!(repo.get_note_by_path("a.md").await.expect("q").is_some());
    assert!(
        repo.get_note_by_path("ghost.md")
            .await
            .expect("q")
            .is_none()
    );
}
