//! Tests for bounded graph traversal (`brain::kg::traverse`).
//!
//! Builds a small graph directly via the repository, then exercises depth
//! limits, dedup, degree/MOC ranking, and budget truncation.

use crate::brain::kg::traverse::traverse;
use crate::db::{Database, KnowledgeGraphRepository, NoteUpsert, RelationInput};

async fn idx(
    repo: &KnowledgeGraphRepository,
    path: &str,
    title: &str,
    note_type: &str,
    relations: &[(&str, &str)],
) -> i64 {
    let rels = relations
        .iter()
        .map(|(t, n)| RelationInput {
            to_name: n.to_string(),
            relation_type: t.to_string(),
            context: None,
        })
        .collect();
    repo.index_note(
        NoteUpsert {
            path: path.to_string(),
            title: title.to_string(),
            note_type: Some(note_type.to_string()),
            frontmatter_json: None,
            body: title.to_string(),
            checksum: path.to_string(),
            mtime: 0,
            size: 0,
        },
        vec![],
        rels,
    )
    .await
    .expect("index")
}

/// MOC → {A, B, C}; A → B; B → C. C and the MOC anchor the graph.
async fn build_graph(repo: &KnowledgeGraphRepository) -> (i64, i64, i64, i64) {
    let moc = idx(
        repo,
        "MOCs/Async.md",
        "Async",
        "moc",
        &[("links_to", "Rust Async"), ("links_to", "Tokio"), ("links_to", "Pinning")],
    )
    .await;
    let a = idx(repo, "concepts/Rust Async.md", "Rust Async", "concept", &[("depends_on", "Tokio")]).await;
    let b = idx(repo, "concepts/Tokio.md", "Tokio", "concept", &[("depends_on", "Pinning")]).await;
    let c = idx(repo, "concepts/Pinning.md", "Pinning", "concept", &[]).await;
    repo.resolve_dangling_links().await.expect("resolve");
    (moc, a, b, c)
}

async fn setup() -> (Database, KnowledgeGraphRepository) {
    let db = Database::connect_in_memory().await.expect("db");
    db.run_migrations().await.expect("migrations");
    let repo = KnowledgeGraphRepository::new(db.pool().clone());
    (db, repo)
}

#[tokio::test]
async fn depth_one_reaches_direct_neighbors() {
    let (_db, repo) = setup().await;
    let (moc, a, b, _c) = build_graph(&repo).await;

    let result = traverse(&repo, &[a], 1, 12).await.expect("traverse");
    let ids: Vec<i64> = result.nodes.iter().map(|n| n.id).collect();
    assert!(ids.contains(&a), "seed present");
    assert!(ids.contains(&b), "outgoing neighbor present");
    assert!(ids.contains(&moc), "backlink neighbor present");
    // C is two hops away — not reached at depth 1.
}

#[tokio::test]
async fn depth_two_reaches_two_hops() {
    let (_db, repo) = setup().await;
    let (_moc, a, _b, c) = build_graph(&repo).await;

    let result = traverse(&repo, &[a], 2, 12).await.expect("traverse");
    let ids: Vec<i64> = result.nodes.iter().map(|n| n.id).collect();
    assert!(ids.contains(&c), "two-hop node reached at depth 2");
}

#[tokio::test]
async fn nodes_are_deduplicated() {
    let (_db, repo) = setup().await;
    let (_moc, a, _b, _c) = build_graph(&repo).await;
    let result = traverse(&repo, &[a], 2, 12).await.expect("traverse");
    let mut ids: Vec<i64> = result.nodes.iter().map(|n| n.id).collect();
    let len_before = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), len_before, "C reachable via two paths appears once");
}

#[tokio::test]
async fn moc_ranks_above_equal_degree_leaf() {
    let (_db, repo) = setup().await;
    let (moc, a, b, _c) = build_graph(&repo).await;

    let result = traverse(&repo, &[a], 1, 12).await.expect("traverse");
    let pos = |id: i64| result.nodes.iter().position(|n| n.id == id);
    let moc_pos = pos(moc).expect("moc present");
    let b_pos = pos(b).expect("b present");
    assert!(moc_pos < b_pos, "MOC hub ranks ahead of an equal-degree leaf");
}

#[tokio::test]
async fn budget_truncates_lowest_ranked() {
    let (_db, repo) = setup().await;
    let (_moc, a, _b, _c) = build_graph(&repo).await;

    let result = traverse(&repo, &[a], 2, 2).await.expect("traverse");
    assert_eq!(result.nodes.len(), 2, "capped to budget");
    assert!(result.truncated, "flagged truncated");
    // The seed (distance 0) always survives truncation.
    assert_eq!(result.nodes[0].id, a);
}

#[tokio::test]
async fn depth_is_clamped_to_max() {
    let (_db, repo) = setup().await;
    let (_moc, a, _b, _c) = build_graph(&repo).await;
    // depth 9 is clamped to 2; should not error or over-expand beyond the graph.
    let result = traverse(&repo, &[a], 9, 50).await.expect("traverse");
    assert!(result.nodes.len() <= 4);
}
