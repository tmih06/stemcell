//! Tests for the `KnowledgeGraphRepository` — the rebuildable SQLite index over
//! the markdown vault. Covers note upsert/replace, FTS entry-point search,
//! typed-relation neighbours/backlinks, dangling-link back-fill (the ghost-node
//! mechanism), prune-on-delete, and degree centrality.

use crate::db::Database;
use crate::db::repository::{
    KnowledgeGraphRepository, LinkDirection, NoteUpsert, ObservationInput, RelationInput,
};

async fn setup() -> (Database, KnowledgeGraphRepository) {
    let db = Database::connect_in_memory()
        .await
        .expect("Failed to create database");
    db.run_migrations().await.expect("Failed to run migrations");
    let repo = KnowledgeGraphRepository::new(db.pool().clone());
    (db, repo)
}

fn note(path: &str, title: &str, body: &str, checksum: &str) -> NoteUpsert {
    NoteUpsert {
        path: path.to_string(),
        title: title.to_string(),
        note_type: Some("concept".to_string()),
        frontmatter_json: Some("{\"type\":\"concept\"}".to_string()),
        body: body.to_string(),
        checksum: checksum.to_string(),
        mtime: 1000,
        size: body.len() as i64,
    }
}

#[tokio::test]
async fn index_and_get_round_trips() {
    let (_db, repo) = setup().await;
    let id = repo
        .index_note(
            note(
                "concepts/Rust Async.md",
                "Rust Async",
                "Futures are lazy",
                "c1",
            ),
            vec![],
            vec![],
        )
        .await
        .expect("index");
    let got = repo
        .get_note_by_path("concepts/Rust Async.md")
        .await
        .expect("query")
        .expect("present");
    assert_eq!(got.id, id);
    assert_eq!(got.title, "Rust Async");
    assert_eq!(got.note_type.as_deref(), Some("concept"));
    assert_eq!(got.checksum, "c1");
}

#[tokio::test]
async fn get_note_by_name_matches_title_case_insensitive() {
    let (_db, repo) = setup().await;
    repo.index_note(
        note("concepts/Rust Async.md", "Rust Async", "body", "c1"),
        vec![],
        vec![],
    )
    .await
    .expect("index");
    let got = repo.get_note_by_name("rust async").await.expect("query");
    assert!(got.is_some(), "title match should be case-insensitive");
    assert_eq!(got.unwrap().title, "Rust Async");
}

#[tokio::test]
async fn get_note_by_name_matches_nested_filename_stem() {
    let (_db, repo) = setup().await;
    // Title differs from filename — lookup by the filename stem must still hit.
    repo.index_note(
        note("concepts/Tokio.md", "The Tokio Runtime", "body", "c1"),
        vec![],
        vec![],
    )
    .await
    .expect("index");
    let got = repo.get_note_by_name("Tokio").await.expect("query");
    assert!(got.is_some(), "should match nested filename stem");
    assert_eq!(got.unwrap().path, "concepts/Tokio.md");
}

#[tokio::test]
async fn fts_search_matches_title_and_body_with_limit() {
    let (_db, repo) = setup().await;
    repo.index_note(
        note("a.md", "Rust Async", "Futures are lazy until polled", "c1"),
        vec![],
        vec![],
    )
    .await
    .expect("a");
    repo.index_note(
        note("b.md", "Tokio Runtime", "Work-stealing scheduler", "c2"),
        vec![],
        vec![],
    )
    .await
    .expect("b");
    repo.index_note(
        note("c.md", "Pinning", "Self-referential futures", "c3"),
        vec![],
        vec![],
    )
    .await
    .expect("c");

    // Title hit
    let by_title = repo.search_fts("tokio", 5).await.expect("search");
    assert_eq!(by_title.len(), 1);
    assert_eq!(by_title[0].title, "Tokio Runtime");

    // Body hit
    let by_body = repo.search_fts("polled", 5).await.expect("search");
    assert_eq!(by_body.len(), 1);
    assert_eq!(by_body[0].title, "Rust Async");

    // Limit cap
    let many = repo.search_fts("futures", 1).await.expect("search");
    assert!(many.len() <= 1);
}

#[tokio::test]
async fn empty_query_returns_no_results() {
    let (_db, repo) = setup().await;
    repo.index_note(note("a.md", "Rust", "body", "c1"), vec![], vec![])
        .await
        .expect("a");
    let got = repo.search_fts("   ", 5).await.expect("search");
    assert!(got.is_empty());
}

#[tokio::test]
async fn observations_round_trip() {
    let (_db, repo) = setup().await;
    let id = repo
        .index_note(
            note("a.md", "Rust Async", "body", "c1"),
            vec![
                ObservationInput {
                    category: Some("fact".to_string()),
                    content: "Futures are lazy".to_string(),
                    tags_json: Some("[\"rust\"]".to_string()),
                    context: None,
                },
                ObservationInput {
                    category: Some("gotcha".to_string()),
                    content: "Holding a Mutex across await deadlocks".to_string(),
                    tags_json: None,
                    context: Some("async runtime".to_string()),
                },
            ],
            vec![],
        )
        .await
        .expect("index");
    let obs = repo.observations_for_note(id).await.expect("obs");
    assert_eq!(obs.len(), 2);
    assert_eq!(obs[0].category.as_deref(), Some("fact"));
    assert_eq!(obs[1].context.as_deref(), Some("async runtime"));
}

#[tokio::test]
async fn outgoing_relation_starts_as_ghost_then_resolves() {
    let (_db, repo) = setup().await;
    let a = repo
        .index_note(
            note("a.md", "Rust Async", "body", "c1"),
            vec![],
            vec![RelationInput {
                to_name: "Tokio Runtime".to_string(),
                relation_type: "depends_on".to_string(),
                context: None,
            }],
        )
        .await
        .expect("a");

    // Before the target exists, the edge is a ghost (to_id NULL).
    let out = repo.neighbors(a, LinkDirection::Out).await.expect("out");
    assert_eq!(out.len(), 1);
    assert!(out[0].outgoing);
    assert_eq!(out[0].relation_type, "depends_on");
    assert_eq!(out[0].other_name, "Tokio Runtime");
    assert!(out[0].other_id.is_none(), "should be a ghost link");

    // Create the target and resolve.
    let b = repo
        .index_note(note("b.md", "Tokio Runtime", "body", "c2"), vec![], vec![])
        .await
        .expect("b");
    let resolved = repo.resolve_dangling_links().await.expect("resolve");
    assert_eq!(resolved, 1);

    let out2 = repo.neighbors(a, LinkDirection::Out).await.expect("out2");
    assert_eq!(out2[0].other_id, Some(b));

    // Backlink now visible from B.
    let back = repo.backlinks(b).await.expect("back");
    assert_eq!(back.len(), 1);
    assert!(!back[0].outgoing);
    assert_eq!(back[0].other_id, Some(a));
    assert_eq!(back[0].relation_type, "depends_on");
}

#[tokio::test]
async fn reindex_replaces_children_and_updates_checksum() {
    let (_db, repo) = setup().await;
    let id1 = repo
        .index_note(
            note("a.md", "Rust", "body", "c1"),
            vec![ObservationInput {
                category: Some("fact".to_string()),
                content: "old".to_string(),
                tags_json: None,
                context: None,
            }],
            vec![RelationInput {
                to_name: "Old Target".to_string(),
                relation_type: "links_to".to_string(),
                context: None,
            }],
        )
        .await
        .expect("first");

    let id2 = repo
        .index_note(
            note("a.md", "Rust", "body", "c2"),
            vec![ObservationInput {
                category: Some("fact".to_string()),
                content: "new".to_string(),
                tags_json: None,
                context: None,
            }],
            vec![],
        )
        .await
        .expect("second");

    assert_eq!(id1, id2, "upsert keeps the same row id");
    let got = repo
        .get_note_by_path("a.md")
        .await
        .expect("q")
        .expect("present");
    assert_eq!(got.checksum, "c2");
    let obs = repo.observations_for_note(id2).await.expect("obs");
    assert_eq!(obs.len(), 1);
    assert_eq!(obs[0].content, "new");
    let out = repo.neighbors(id2, LinkDirection::Out).await.expect("out");
    assert!(out.is_empty(), "old relation replaced");
}

#[tokio::test]
async fn prune_missing_removes_notes_and_reverts_incoming_to_ghost() {
    let (_db, repo) = setup().await;
    let a = repo
        .index_note(
            note("a.md", "Rust Async", "body", "c1"),
            vec![],
            vec![RelationInput {
                to_name: "Tokio Runtime".to_string(),
                relation_type: "depends_on".to_string(),
                context: None,
            }],
        )
        .await
        .expect("a");
    repo.index_note(note("b.md", "Tokio Runtime", "body", "c2"), vec![], vec![])
        .await
        .expect("b");
    repo.resolve_dangling_links().await.expect("resolve");

    // Prune everything except a.md.
    let pruned = repo
        .prune_missing(&["a.md".to_string()])
        .await
        .expect("prune");
    assert_eq!(pruned, 1);
    assert_eq!(repo.note_count().await.expect("count"), 1);

    // A's edge to the now-deleted B reverts to ghost.
    let out = repo.neighbors(a, LinkDirection::Out).await.expect("out");
    assert_eq!(out.len(), 1);
    assert!(out[0].other_id.is_none(), "deleted target → ghost again");
}

#[tokio::test]
async fn prune_missing_empty_keep_set_removes_all() {
    let (_db, repo) = setup().await;
    repo.index_note(note("a.md", "A", "body", "c1"), vec![], vec![])
        .await
        .expect("a");
    repo.index_note(note("b.md", "B", "body", "c2"), vec![], vec![])
        .await
        .expect("b");
    let pruned = repo.prune_missing(&[]).await.expect("prune");
    assert_eq!(pruned, 2);
    assert_eq!(repo.note_count().await.expect("count"), 0);
}

#[tokio::test]
async fn delete_note_by_path_works() {
    let (_db, repo) = setup().await;
    repo.index_note(note("a.md", "A", "body", "c1"), vec![], vec![])
        .await
        .expect("a");
    assert!(repo.delete_note_by_path("a.md").await.expect("del"));
    assert!(!repo.delete_note_by_path("a.md").await.expect("del2"));
    assert_eq!(repo.note_count().await.expect("count"), 0);
}

#[tokio::test]
async fn degree_counts_in_and_out_edges() {
    let (_db, repo) = setup().await;
    let hub = repo
        .index_note(
            note("hub.md", "Hub", "body", "c1"),
            vec![],
            vec![RelationInput {
                to_name: "Leaf".to_string(),
                relation_type: "links_to".to_string(),
                context: None,
            }],
        )
        .await
        .expect("hub");
    let leaf = repo
        .index_note(
            note("leaf.md", "Leaf", "body", "c2"),
            vec![],
            vec![RelationInput {
                to_name: "Hub".to_string(),
                relation_type: "links_to".to_string(),
                context: None,
            }],
        )
        .await
        .expect("leaf");
    repo.resolve_dangling_links().await.expect("resolve");

    // hub: 1 outgoing (→leaf) + 1 incoming (leaf→hub) = 2
    assert_eq!(repo.degree(hub).await.expect("deg"), 2);
    assert_eq!(repo.degree(leaf).await.expect("deg"), 2);
}
