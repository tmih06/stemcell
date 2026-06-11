//! `kg_search` — knowledge-graph entry-point search.
//!
//! Returns ranked entry points (`title · path · 1-line snippet · type`) for a
//! query. No note bodies — this is the cheap first hop of the retrieve-then-
//! traverse pattern; follow up with `kg_context`/`kg_links`/`kg_read`.

use super::error::Result;
use super::r#trait::{Tool, ToolCapability, ToolExecutionContext, ToolResult};
use crate::db::KnowledgeGraphRepository;
use async_trait::async_trait;
use serde_json::Value;

pub struct KgSearchTool {
    repo: KnowledgeGraphRepository,
}

impl KgSearchTool {
    pub fn new(repo: KnowledgeGraphRepository) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl Tool for KgSearchTool {
    fn name(&self) -> &str {
        "kg_search"
    }

    fn description(&self) -> &str {
        "Search the knowledge-graph vault for entry-point notes. Returns ranked \
         title · path · type · 1-line snippet (no bodies). Use this first to find \
         where to start, then kg_context/kg_links to expand or kg_read for specifics."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Keywords to search note titles, bodies, and observations"
                },
                "n": {
                    "type": "integer",
                    "description": "Max entry points to return (default: 5)",
                    "default": 5
                }
            },
            "required": ["query"]
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
            .unwrap_or("")
            .trim()
            .to_string();
        if query.is_empty() {
            return Ok(ToolResult::error("query parameter is required".to_string()));
        }
        let n = input.get("n").and_then(|v| v.as_u64()).unwrap_or(5).clamp(1, 25) as usize;

        match self.repo.search_fts(&query, n).await {
            Ok(hits) if hits.is_empty() => Ok(ToolResult::success(format!(
                "No knowledge-graph notes matched \"{query}\"."
            ))),
            Ok(hits) => {
                let mut out = format!("Knowledge-graph entry points for \"{query}\":\n");
                for (i, h) in hits.iter().enumerate() {
                    let kind = h.note_type.as_deref().unwrap_or("note");
                    out.push_str(&format!(
                        "{}. {} · {} · [{}]\n   {}\n",
                        i + 1,
                        h.title,
                        h.path,
                        kind,
                        h.snippet.trim()
                    ));
                }
                Ok(ToolResult::success(out))
            }
            Err(e) => Ok(ToolResult::error(format!("kg_search failed: {e}"))),
        }
    }
}
