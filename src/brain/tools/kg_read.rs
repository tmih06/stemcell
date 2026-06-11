//! `kg_read` — read a knowledge-graph note (or just an anchor of it).
//!
//! Returns the note's frontmatter facts plus its body. When an `anchor`
//! (`#heading` or `^block`) or `section` is given, only that slice is returned —
//! so the agent can pull a specific fragment without loading the whole file.

use super::error::Result;
use super::r#trait::{Tool, ToolCapability, ToolExecutionContext, ToolResult};
use crate::brain::kg::parser;
use crate::brain::kg::resolver;
use crate::brain::kg::vault::Vault;
use crate::db::KnowledgeGraphRepository;
use async_trait::async_trait;
use serde_json::Value;

/// Cap returned body length to keep tool output lean.
const MAX_BODY_CHARS: usize = 6000;

pub struct KgReadTool {
    repo: KnowledgeGraphRepository,
    vault: Vault,
}

impl KgReadTool {
    pub fn new(repo: KnowledgeGraphRepository, vault: Vault) -> Self {
        Self { repo, vault }
    }
}

#[async_trait]
impl Tool for KgReadTool {
    fn name(&self) -> &str {
        "kg_read"
    }

    fn description(&self) -> &str {
        "Read a knowledge-graph note by name or path. Returns frontmatter facts \
         plus the body. Pass `anchor` (a #heading or ^block-id) or `section` to \
         slice-read just that fragment instead of the whole note."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "note": {
                    "type": "string",
                    "description": "Note name (e.g. \"Rust Async\") or vault-relative path (e.g. \"concepts/Rust Async.md\")"
                },
                "anchor": {
                    "type": "string",
                    "description": "Optional heading text or ^block-id to slice-read just that section"
                },
                "section": {
                    "type": "string",
                    "description": "Optional heading to read (alias for a heading anchor)"
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
        let anchor = input
            .get("anchor")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let section = input
            .get("section")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let record = match self.repo.get_note_by_ref(&note_ref).await {
            Ok(Some(r)) => r,
            Ok(None) => {
                return Ok(ToolResult::success(format!(
                    "No note found for \"{note_ref}\". Try kg_search to find the right name."
                )));
            }
            Err(e) => return Ok(ToolResult::error(format!("kg_read failed: {e}"))),
        };

        let content = match self.vault.read_note(&record.path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Indexed note {} could not be read from disk: {e}",
                    record.path
                )));
            }
        };

        let parsed = parser::parse(&content);
        let mut out = String::new();
        out.push_str(&format!("# {}", record.title));
        if let Some(t) = &record.note_type {
            out.push_str(&format!("  ({t})"));
        }
        out.push_str(&format!("\n_{}_\n", record.path));
        if !parsed.frontmatter.tags.is_empty() {
            out.push_str(&format!("tags: {}\n", parsed.frontmatter.tags.join(", ")));
        }
        if !parsed.frontmatter.aliases.is_empty() {
            out.push_str(&format!("aliases: {}\n", parsed.frontmatter.aliases.join(", ")));
        }
        out.push_str("\n");

        // Resolve an anchor/section to a slice if requested.
        let slice = if let Some(a) = anchor {
            if let Some(block) = a.strip_prefix('^') {
                resolver::block_range(&content, block)
            } else {
                resolver::heading_range(&content, a)
            }
        } else {
            section.and_then(|s| resolver::heading_range(&content, s))
        };

        let body = match (anchor.or(section), slice) {
            (Some(label), Some(range)) => {
                format!("(section: {label})\n{}", resolver::slice_lines(&content, range))
            }
            (Some(label), None) => {
                format!(
                    "(anchor \"{label}\" not found — showing full note)\n{}",
                    parser::body_after_frontmatter(&content)
                )
            }
            (None, _) => parser::body_after_frontmatter(&content),
        };

        let body = body.trim();
        if body.chars().count() > MAX_BODY_CHARS {
            let truncated: String = body.chars().take(MAX_BODY_CHARS).collect();
            out.push_str(&truncated);
            out.push_str("\n…(truncated — use `anchor`/`section` to read a specific part)");
        } else {
            out.push_str(body);
        }

        Ok(ToolResult::success(out))
    }
}
