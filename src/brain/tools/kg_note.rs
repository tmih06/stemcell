//! `kg_note` — capture or extend a knowledge-graph note.
//!
//! Writes/updates a markdown note in the vault and reindexes it. Updates are
//! *surgical*: new observations/relations are appended to the relevant sections,
//! never a full rewrite, so user-authored content is preserved. New notes get a
//! frontmatter header and are filed into the folder for their type.

use super::error::Result;
use super::r#trait::{Tool, ToolCapability, ToolExecutionContext, ToolResult};
use crate::brain::kg::sync;
use crate::brain::kg::vault::Vault;
use crate::db::KnowledgeGraphRepository;
use async_trait::async_trait;
use serde_json::Value;

// Note-composition helpers live in the ungated `brain::kg::compose` layer so the
// gated `kg_remember` tool and the git-review service can reuse them without
// depending on the `tool-kg-note` feature. Re-exported here for callers (and
// tests) that reference the historical `kg_note::` path.
pub use crate::brain::kg::compose::{
    build_note, insert_bullets, observation_bullet, relation_bullet, resolve_note_rel,
};

pub struct KgNoteTool {
    repo: KnowledgeGraphRepository,
    vault: Vault,
}

impl KgNoteTool {
    pub fn new(repo: KnowledgeGraphRepository, vault: Vault) -> Self {
        Self { repo, vault }
    }
}

#[async_trait]
impl Tool for KgNoteTool {
    fn name(&self) -> &str {
        "kg_note"
    }

    fn description(&self) -> &str {
        "Create or extend a knowledge-graph note in the vault. Appends \
         observations and relations surgically (never rewrites existing content) \
         and reindexes. Use it to durably remember a fact, decision, or entity \
         with typed links to related notes. Relations are objects \
         {\"type\": \"depends_on\", \"target\": \"Tokio Runtime\"}."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Note title (also the wikilink target name)"
                },
                "type": {
                    "type": "string",
                    "description": "Note type: concept | person | project | moc | daily (default: concept)"
                },
                "observations": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Atomic facts, e.g. \"[fact] Futures are lazy #rust\""
                },
                "relations": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "type": { "type": "string", "description": "Relation type, e.g. depends_on" },
                            "target": { "type": "string", "description": "Target note name" }
                        },
                        "required": ["target"]
                    },
                    "description": "Typed links to other notes"
                },
                "mode": {
                    "type": "string",
                    "enum": ["create", "append"],
                    "description": "create (default) makes a new note or appends if it exists; append requires it to exist",
                    "default": "create"
                }
            },
            "required": ["title"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadFiles, ToolCapability::WriteFiles]
    }

    async fn execute(&self, input: Value, _context: &ToolExecutionContext) -> Result<ToolResult> {
        let title = input
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if title.is_empty() {
            return Ok(ToolResult::error("title parameter is required".to_string()));
        }
        let note_type = input
            .get("type")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let mode = input
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("create");

        let observation_bullets: Vec<String> = input
            .get("observations")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .map(observation_bullet)
                    .collect()
            })
            .unwrap_or_default();

        let relation_bullets: Vec<String> = input
            .get("relations")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(relation_bullet).collect())
            .unwrap_or_default();

        if observation_bullets.is_empty() && relation_bullets.is_empty() {
            return Ok(ToolResult::error(
                "provide at least one observation or relation".to_string(),
            ));
        }

        // Resolve the target path: an existing note (by title) keeps its path;
        // otherwise file it under the folder for its type.
        let rel = match resolve_note_rel(&self.repo, &title, note_type.as_deref()).await {
            Ok(rel) => rel,
            Err(e) => return Ok(ToolResult::error(format!("kg_note failed: {e}"))),
        };

        let exists = self.vault.exists(&rel);
        if mode == "append" && !exists {
            return Ok(ToolResult::error(format!(
                "mode=append but no note exists for \"{title}\" — use mode=create"
            )));
        }

        let (content, added_obs, added_rel) = if exists {
            let mut content = self.vault.read_note(&rel).unwrap_or_default();
            let added_obs;
            let added_rel;
            (content, added_obs) = insert_bullets(&content, "Observations", &observation_bullets);
            (content, added_rel) = insert_bullets(&content, "Relations", &relation_bullets);
            (content, added_obs, added_rel)
        } else {
            // A fresh note inserts every bullet verbatim, so the input counts are
            // exactly what was added.
            (
                build_note(
                    &title,
                    note_type.as_deref(),
                    &observation_bullets,
                    &relation_bullets,
                ),
                observation_bullets.len(),
                relation_bullets.len(),
            )
        };

        if let Err(e) = self.vault.write_note(&rel, &content) {
            return Ok(ToolResult::error(format!("Failed to write {rel}: {e}")));
        }

        // Reindex just this note so the graph reflects the write immediately.
        if let Err(e) = sync::index_file(&self.vault, &self.repo, &rel).await {
            tracing::warn!("kg_note: wrote {rel} but reindex failed: {e}");
        }

        let action = if exists { "Updated" } else { "Created" };
        Ok(ToolResult::success(format!(
            "{action} {rel} (+{added_obs} observation(s), +{added_rel} relation(s))"
        )))
    }
}
