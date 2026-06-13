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
async fn fts_search_or_joins_tokens_so_partial_matches_hit() {
    let (_db, repo) = setup().await;
    // No single note contains every token of the natural-language query, so an
    // AND-join would return nothing. OR-join must still surface the matches.
    repo.index_note(
        note("a.md", "Tokio Runtime", "Work-stealing scheduler", "c1"),
        vec![],
        vec![],
    )
    .await
    .expect("a");
    repo.index_note(
        note("b.md", "Tasks", "How futures become tasks", "c2"),
        vec![],
        vec![],
    )
    .await
    .expect("b");

    let hits = repo
        .search_fts("how does tokio schedule tasks", 5)
        .await
        .expect("search");
    let titles: Vec<&str> = hits.iter().map(|h| h.title.as_str()).collect();
    assert!(
        titles.contains(&"Tokio Runtime"),
        "OR semantics should match the note containing only 'tokio': {titles:?}"
    );
    assert!(
        titles.contains(&"Tasks"),
        "OR semantics should match the note containing only 'tasks': {titles:?}"
    );
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
async fn resolve_links_for_note_resolves_both_directions() {
    let (_db, repo) = setup().await;

    // (b) back-fill direction: B exists first and links to a not-yet-created A.
    let b = repo
        .index_note(
            note("b.md", "Tokio Runtime", "body", "c2"),
            vec![],
            vec![RelationInput {
                to_name: "Rust Async".to_string(),
                relation_type: "used_by".to_string(),
                context: None,
            }],
        )
        .await
        .expect("b");
    // B's link is a ghost — A does not exist yet.
    let b_out = repo.neighbors(b, LinkDirection::Out).await.expect("b out");
    assert!(b_out[0].other_id.is_none());

    // Now create A, which itself links forward to B (the (a) outgoing direction).
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

    // Scoped resolve for A: resolves A→B (outgoing) and back-fills B→A (incoming).
    let resolved = repo.resolve_links_for_note(a).await.expect("resolve");
    assert_eq!(
        resolved, 2,
        "both A's outgoing and B's incoming ghost resolve"
    );

    // (a) A's outgoing edge now points at B.
    let a_out = repo.neighbors(a, LinkDirection::Out).await.expect("a out");
    assert_eq!(a_out[0].other_id, Some(b));

    // (b) B's pre-existing ghost now points at A.
    let b_out2 = repo.neighbors(b, LinkDirection::Out).await.expect("b out2");
    assert_eq!(b_out2[0].other_id, Some(a));
}

#[tokio::test]
async fn resolve_links_for_note_back_fills_by_filename_stem() {
    let (_db, repo) = setup().await;
    // A links to "Tokio" (a filename stem, not the title of the target note).
    repo.index_note(
        note("a.md", "Rust Async", "body", "c1"),
        vec![],
        vec![RelationInput {
            to_name: "Tokio".to_string(),
            relation_type: "depends_on".to_string(),
            context: None,
        }],
    )
    .await
    .expect("a");
    // Target note's title differs from its filename stem.
    let b = repo
        .index_note(
            note("concepts/Tokio.md", "The Tokio Runtime", "body", "c2"),
            vec![],
            vec![],
        )
        .await
        .expect("b");

    let resolved = repo.resolve_links_for_note(b).await.expect("resolve");
    assert_eq!(resolved, 1, "ghost matching the filename stem back-fills");
    let back = repo.backlinks(b).await.expect("back");
    assert_eq!(back.len(), 1);
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
async fn prune_paths_removes_notes_and_reverts_incoming_to_ghost() {
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

    // Prune b.md by naming it explicitly.
    let pruned = repo
        .prune_paths(&["b.md".to_string()])
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
async fn prune_paths_empty_input_is_a_noop() {
    let (_db, repo) = setup().await;
    repo.index_note(note("a.md", "A", "body", "c1"), vec![], vec![])
        .await
        .expect("a");
    repo.index_note(note("b.md", "B", "body", "c2"), vec![], vec![])
        .await
        .expect("b");
    // An empty doomed set deletes nothing — unlike the old keep-set prune, this
    // can never wipe the index, which is what makes the snapshot-diff caller in
    // `reindex` safe against a concurrently-emptied filesystem listing.
    let pruned = repo.prune_paths(&[]).await.expect("prune");
    assert_eq!(pruned, 0);
    assert_eq!(repo.note_count().await.expect("count"), 2);
}

#[tokio::test]
async fn prune_paths_removes_only_named_notes() {
    let (_db, repo) = setup().await;
    repo.index_note(note("a.md", "A", "body", "c1"), vec![], vec![])
        .await
        .expect("a");
    repo.index_note(note("b.md", "B", "body", "c2"), vec![], vec![])
        .await
        .expect("b");
    repo.index_note(note("c.md", "C", "body", "c3"), vec![], vec![])
        .await
        .expect("c");
    // Naming a missing path alongside real ones is harmless — only the present
    // ones are removed, and the count reflects exactly that.
    let pruned = repo
        .prune_paths(&[
            "a.md".to_string(),
            "c.md".to_string(),
            "gone.md".to_string(),
        ])
        .await
        .expect("prune");
    assert_eq!(pruned, 2);
    assert_eq!(repo.note_count().await.expect("count"), 1);
    assert!(repo.get_note_by_path("b.md").await.expect("q").is_some());
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
