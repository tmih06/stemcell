//! Bounded-depth graph traversal for summary-first retrieval.
//!
//! Starting from one or more seed notes, walk the typed-relation graph up to a
//! small hop limit (1–2), dedup by note id, rank the reached notes by
//! degree-centrality (a cheap PageRank approximation) with a preference for
//! MOC/hub notes, and truncate to a hard node budget — lowest-ranked first.
//! The `kg_context` tool renders the result without dumping whole files.

use crate::db::KnowledgeGraphRepository;
use crate::db::repository::{LinkDirection, Neighbor};
use anyhow::Result;
use std::collections::HashMap;

/// Hard upper bound on traversal depth regardless of caller request.
pub const MAX_DEPTH: usize = 2;
/// Default node budget when the caller doesn't specify one.
pub const DEFAULT_MAX_NODES: usize = 12;

/// A note reached during traversal.
#[derive(Debug, Clone)]
pub struct GraphNode {
    pub id: i64,
    pub title: String,
    pub path: String,
    pub note_type: Option<String>,
    /// Hops from the nearest seed (0 = seed).
    pub distance: usize,
    /// Total degree (in + out edges) — the centrality signal.
    pub degree: i64,
    /// Outgoing edges as `(relation_type, other_name)`, captured during the
    /// single `neighbors(Both)` fetch done when this node was added. Lets
    /// `kg_context` render links without a second per-node neighbors query.
    pub outgoing: Vec<(String, String)>,
}

/// The ranked, budget-capped result of a traversal.
#[derive(Debug, Clone, Default)]
pub struct TraverseResult {
    pub nodes: Vec<GraphNode>,
    /// True if the budget dropped lower-ranked nodes.
    pub truncated: bool,
}

/// Walk the graph from `seeds` out to `depth` hops, returning ranked notes.
pub async fn traverse(
    repo: &KnowledgeGraphRepository,
    seeds: &[i64],
    depth: usize,
    max_nodes: usize,
) -> Result<TraverseResult> {
    let depth = depth.min(MAX_DEPTH);
    let max_nodes = max_nodes.max(1);

    let mut visited: HashMap<i64, GraphNode> = HashMap::new();
    // Frontier carries each node's already-fetched neighbors so we expand
    // without a second neighbors() call per node.
    let mut frontier: Vec<Vec<Neighbor>> = Vec::new();

    for &seed in seeds {
        if let Some(neighbors) = add_node(repo, &mut visited, seed, 0).await? {
            frontier.push(neighbors);
        }
    }

    for d in 1..=depth {
        if frontier.is_empty() {
            break;
        }
        let mut next = Vec::new();
        for neighbors in &frontier {
            for nb in neighbors {
                if let Some(other) = nb.other_id
                    && !visited.contains_key(&other)
                    && let Some(other_neighbors) = add_node(repo, &mut visited, other, d).await?
                {
                    next.push(other_neighbors);
                }
            }
        }
        frontier = next;
    }

    let mut nodes: Vec<GraphNode> = visited.into_values().collect();
    nodes.sort_by(|a, b| {
        a.distance
            .cmp(&b.distance)
            .then_with(|| score(b).cmp(&score(a)))
            .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });

    let truncated = nodes.len() > max_nodes;
    nodes.truncate(max_nodes);
    Ok(TraverseResult { nodes, truncated })
}

/// Ranking score: degree centrality plus a large MOC/hub bonus so map-of-content
/// notes surface ahead of equally-connected leaves.
fn score(n: &GraphNode) -> i64 {
    let moc_bonus = if n.note_type.as_deref() == Some("moc") {
        1000
    } else {
        0
    };
    n.degree + moc_bonus
}

/// Fetch a note and its edges, inserting it into `visited`. Returns the node's
/// `neighbors(Both)` for the caller to expand the frontier, or `None` if the
/// node was already visited or no longer exists.
///
/// One `neighbors(Both)` call replaces both the old `degree` query and the
/// per-node render-time `neighbors(Out)` call: `degree` (in + out edges) equals
/// the neighbor count because `neighbors(Both)` returns exactly the rows
/// `repo.degree` counts (`from_id = id` plus `to_id = id`), and the outgoing
/// subset is stashed on the node for `kg_context` to render.
async fn add_node(
    repo: &KnowledgeGraphRepository,
    visited: &mut HashMap<i64, GraphNode>,
    id: i64,
    distance: usize,
) -> Result<Option<Vec<Neighbor>>> {
    if visited.contains_key(&id) {
        return Ok(None);
    }
    let Some(rec) = repo.get_note_by_id(id).await? else {
        return Ok(None);
    };
    let neighbors = repo.neighbors(id, LinkDirection::Both).await?;
    let degree = neighbors.len() as i64;
    let outgoing = neighbors
        .iter()
        .filter(|nb| nb.outgoing)
        .map(|nb| (nb.relation_type.clone(), nb.other_name.clone()))
        .collect();
    visited.insert(
        id,
        GraphNode {
            id,
            title: rec.title,
            path: rec.path,
            note_type: rec.note_type,
            distance,
            degree,
            outgoing,
        },
    );
    Ok(Some(neighbors))
}
