//! Bounded-depth graph traversal for summary-first retrieval.
//!
//! Starting from one or more seed notes, walk the typed-relation graph up to a
//! small hop limit (1–2), dedup by note id, rank the reached notes by
//! degree-centrality (a cheap PageRank approximation) with a preference for
//! MOC/hub notes, and truncate to a hard node budget — lowest-ranked first.
//! The `kg_context` tool renders the result without dumping whole files.

use crate::db::KnowledgeGraphRepository;
use crate::db::repository::LinkDirection;
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
    let mut frontier: Vec<i64> = Vec::new();

    for &seed in seeds {
        if add_node(repo, &mut visited, seed, 0).await? {
            frontier.push(seed);
        }
    }

    for d in 1..=depth {
        if frontier.is_empty() {
            break;
        }
        let mut next = Vec::new();
        for &node_id in &frontier {
            for nb in repo.neighbors(node_id, LinkDirection::Both).await? {
                if let Some(other) = nb.other_id
                    && !visited.contains_key(&other)
                    && add_node(repo, &mut visited, other, d).await?
                {
                    next.push(other);
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

async fn add_node(
    repo: &KnowledgeGraphRepository,
    visited: &mut HashMap<i64, GraphNode>,
    id: i64,
    distance: usize,
) -> Result<bool> {
    if visited.contains_key(&id) {
        return Ok(false);
    }
    let Some(rec) = repo.get_note_by_id(id).await? else {
        return Ok(false);
    };
    let degree = repo.degree(id).await?;
    visited.insert(
        id,
        GraphNode {
            id,
            title: rec.title,
            path: rec.path,
            note_type: rec.note_type,
            distance,
            degree,
        },
    );
    Ok(true)
}
