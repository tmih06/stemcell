//! `kg_links` — show a note's typed relations and backlinks.
//!
//! Returns outgoing edges (`relation_type → [[Target]]`) and/or backlinks
//! (`[[Source]] → relation_type`) — link lines only, no bodies. Unresolved
//! (ghost) targets are flagged.

use super::error::Result;
use super::r#trait::{Tool, ToolCapability, ToolExecutionContext, ToolResult};
use crate::db::KnowledgeGraphRepository;
use crate::db::repository::LinkDirection;
use async_trait::async_trait;
use serde_json::Value;

pub struct KgLinksTool {
    repo: KnowledgeGraphRepository,
}

impl KgLinksTool {
    pub fn new(repo: KnowledgeGraphRepository) -> Self {
        Self { repo }
    }
}

#[async_trait]
impl Tool for KgLinksTool {
    fn name(&self) -> &str {
        "kg_links"
    }

    fn description(&self) -> &str {
        "List a knowledge-graph note's typed relations and backlinks as \
         `relation_type → [[Target]]` lines (no bodies). direction = out | in | both."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "note": {
                    "type": "string",
                    "description": "Note name or vault-relative path"
                },
                "direction": {
                    "type": "string",
                    "enum": ["out", "in", "both"],
                    "description": "out = outgoing relations, in = backlinks, both (default)",
                    "default": "both"
                }
            },
            "required": ["note"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadFiles]
    }

    fn requires_approval(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value, _context: &ToolExecutionContext) -> Result<ToolResult> {
        let note_ref = input
            .get("note")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if note_ref.is_empty() {
            return Ok(ToolResult::error("note parameter is required".to_string()));
        }
        let direction = match input
            .get("direction")
            .and_then(|v| v.as_str())
            .unwrap_or("both")
        {
            "out" => LinkDirection::Out,
            "in" => LinkDirection::In,
            _ => LinkDirection::Both,
        };

        let record = match self.repo.get_note_by_ref(&note_ref).await {
            Ok(Some(r)) => r,
            Ok(None) => {
                return Ok(ToolResult::success(format!(
                    "No note found for \"{note_ref}\". Try kg_search to find the right name."
                )));
            }
            Err(e) => return Ok(ToolResult::error(format!("kg_links failed: {e}"))),
        };

        let neighbors = match self.repo.neighbors(record.id, direction).await {
            Ok(n) => n,
            Err(e) => return Ok(ToolResult::error(format!("kg_links failed: {e}"))),
        };

        let mut out = format!("Links for {} ({}):\n", record.title, record.path);
        let outgoing: Vec<_> = neighbors.iter().filter(|n| n.outgoing).collect();
        let incoming: Vec<_> = neighbors.iter().filter(|n| !n.outgoing).collect();

        if matches!(direction, LinkDirection::Out | LinkDirection::Both) {
            out.push_str("\nOutgoing:\n");
            if outgoing.is_empty() {
                out.push_str("  (none)\n");
            }
            for n in &outgoing {
                let ghost = if n.other_id.is_none() { "  (unresolved)" } else { "" };
                out.push_str(&format!(
                    "  {} → [[{}]]{}\n",
                    n.relation_type, n.other_name, ghost
                ));
            }
        }

        if matches!(direction, LinkDirection::In | LinkDirection::Both) {
            out.push_str("\nBacklinks:\n");
            if incoming.is_empty() {
                out.push_str("  (none)\n");
            }
            for n in &incoming {
                out.push_str(&format!(
                    "  [[{}]] → {}\n",
                    n.other_name, n.relation_type
                ));
            }
        }

        Ok(ToolResult::success(out))
    }
}
