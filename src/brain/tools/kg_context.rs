//! `kg_context` — summary-first bounded graph traversal.
//!
//! Seeds from a `query` (FTS) or a specific `note`, walks the graph up to
//! `depth` hops (max 2), ranks reached notes by centrality, and renders a
//! compact digest — ranked titles + a couple of key facts + outgoing links per
//! note — under a hard node budget. It deliberately does not dump full files;
//! follow up with `kg_read` to expand a specific note.

use super::error::Result;
use super::r#trait::{Tool, ToolCapability, ToolExecutionContext, ToolResult};
use crate::brain::kg::traverse::{self, DEFAULT_MAX_NODES, MAX_DEPTH};
use crate::db::KnowledgeGraphRepository;
use crate::db::repository::LinkDirection;
use async_trait::async_trait;
use serde_json::Value;

/// Max key facts rendered per node.
const FACTS_PER_NODE: usize = 2;
/// Max outgoing links rendered per node.
const LINKS_PER_NODE: usize = 4;
/// Truncation length for a single rendered fact.
const FACT_CHARS: usize = 160;

pub struct KgContextTool {
    repo: KnowledgeGraphRepository,
}

impl KgContextTool {
    pub fn new(repo: KnowledgeGraphRepository) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl Tool for KgContextTool {
    fn name(&self) -> &str {
        "kg_context"
    }

    fn description(&self) -> &str {
        "Summary-first knowledge-graph context. Seed from `query` or `note`, walk \
         the graph up to `depth` hops (max 2), and return ranked notes with key \
         facts and links — without dumping whole files. Use it to gather \
         multi-hop context, then kg_read to expand a specific note."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Seed the traversal from notes matching this search"
                },
                "note": {
                    "type": "string",
                    "description": "Seed the traversal from this specific note (name or path)"
                },
                "depth": {
                    "type": "integer",
                    "description": "Hops to expand (default 1, max 2)",
                    "default": 1
                },
                "budget": {
                    "type": "integer",
                    "description": "Max notes to include (default 12)",
                    "default": 12
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadFiles]
    }

    fn requires_approval(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value, _context: &ToolExecutionContext) -> Result<ToolResult> {
        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let note = input
            .get("note")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let depth = input
            .get("depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .min(MAX_DEPTH as u64) as usize;
        let budget = input
            .get("budget")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_MAX_NODES as u64)
            .clamp(1, 30) as usize;

        // Build the seed set.
        let (seeds, label) = match (note, query) {
            (Some(n), _) => match self.repo.get_note_by_ref(n).await {
                Ok(Some(rec)) => (vec![rec.id], format!("note \"{}\"", rec.title)),
                Ok(None) => {
                    return Ok(ToolResult::success(format!(
                        "No note found for \"{n}\". Try kg_search."
                    )));
                }
                Err(e) => return Ok(ToolResult::error(format!("kg_context failed: {e}"))),
            },
            (None, Some(q)) => match self.repo.search_fts(q, 5).await {
                Ok(hits) if hits.is_empty() => {
                    return Ok(ToolResult::success(format!(
                        "No knowledge-graph notes matched \"{q}\"."
                    )));
                }
                Ok(hits) => (
                    hits.iter().map(|h| h.note_id).collect::<Vec<_>>(),
                    format!("\"{q}\""),
                ),
                Err(e) => return Ok(ToolResult::error(format!("kg_context failed: {e}"))),
            },
            (None, None) => {
                return Ok(ToolResult::error(
                    "provide either `query` or `note`".to_string(),
                ));
            }
        };

        let result = match traverse::traverse(&self.repo, &seeds, depth, budget).await {
            Ok(r) => r,
            Err(e) => return Ok(ToolResult::error(format!("kg_context failed: {e}"))),
        };

        if result.nodes.is_empty() {
            return Ok(ToolResult::success(format!(
                "No graph context found for {label}."
            )));
        }

        let mut out = format!(
            "Knowledge-graph context for {label} — depth {depth}, {} note(s){}:\n",
            result.nodes.len(),
            if result.truncated {
                " (budget-truncated)"
            } else {
                ""
            }
        );

        for node in &result.nodes {
            let kind = node.note_type.as_deref().unwrap_or("note");
            out.push_str(&format!("\n• {} ({}) — {}\n", node.title, kind, node.path));

            // Key facts.
            if let Ok(obs) = self.repo.observations_for_note(node.id).await {
                for o in obs.iter().take(FACTS_PER_NODE) {
                    let mut fact = o.content.trim().to_string();
                    if fact.chars().count() > FACT_CHARS {
                        fact = fact.chars().take(FACT_CHARS).collect::<String>() + "…";
                    }
                    if !fact.is_empty() {
                        out.push_str(&format!("    fact: {fact}\n"));
                    }
                }
            }

            // Outgoing links.
            if let Ok(neighbors) = self.repo.neighbors(node.id, LinkDirection::Out).await {
                let links: Vec<String> = neighbors
                    .iter()
                    .take(LINKS_PER_NODE)
                    .map(|n| format!("{} → [[{}]]", n.relation_type, n.other_name))
                    .collect();
                if !links.is_empty() {
                    out.push_str(&format!("    links: {}\n", links.join("; ")));
                }
            }
        }

        out.push_str("\n(Use kg_read \"<note>\" to expand, kg_links \"<note>\" for all edges.)");
        Ok(ToolResult::success(out))
    }
}
