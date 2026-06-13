//! `kg_remember` — gated batch capture into the knowledge-graph review queue.
//!
//! The review-gated counterpart of `kg_note`. Instead of writing straight to the
//! vault, one call composes *all* the notes the agent wants to remember (correct
//! markdown, links, surgical appends), seals them onto a `kg/batch/<id>` git
//! branch in a sibling worktree, and parks a `pending` row. The user reviews and
//! approves/declines via `/kg`. Nothing reaches long-term memory (or `kg_search`)
//! until approval merges the branch into main.
//!
//! When review mode is on this tool is registered *instead of* `kg_note` for the
//! main agent, so the agent cannot bypass the gate.

use super::error::Result;
use super::r#trait::{Tool, ToolCapability, ToolExecutionContext, ToolResult};
use crate::brain::kg::review::{self, NoteInput};
use crate::config::Config;
use crate::db::Pool;
use async_trait::async_trait;
use serde_json::Value;

pub struct KgRememberTool {
    config: Config,
    pool: Pool,
}

impl KgRememberTool {
    pub fn new(config: Config, pool: Pool) -> Self {
        Self { config, pool }
    }
}

#[async_trait]
impl Tool for KgRememberTool {
    fn name(&self) -> &str {
        "kg_remember"
    }

    fn description(&self) -> &str {
        "Remember a set of facts in long-term memory as linked knowledge-graph \
         notes. Pass everything worth keeping from the conversation in ONE call: \
         each note has a title, type, atomic observations, and typed relations to \
         other notes. The writes are staged on a review branch and queued for the \
         user to approve via /kg — they do NOT take effect until approved. Search \
         first (kg_search) to extend existing notes rather than duplicate them."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "summary": {
                    "type": "string",
                    "description": "One-line description of this batch, shown in the review queue"
                },
                "notes": {
                    "type": "array",
                    "description": "The notes to create or extend in this batch",
                    "items": {
                        "type": "object",
                        "properties": {
                            "title": { "type": "string", "description": "Note title (also the wikilink target name)" },
                            "type": { "type": "string", "description": "concept | person | project | moc | daily (default: concept)" },
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
                            }
                        },
                        "required": ["title"]
                    }
                }
            },
            "required": ["summary", "notes"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadFiles, ToolCapability::WriteFiles]
    }

    async fn execute(&self, input: Value, _context: &ToolExecutionContext) -> Result<ToolResult> {
        let summary = input
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if summary.is_empty() {
            return Ok(ToolResult::error(
                "summary parameter is required".to_string(),
            ));
        }

        let Some(note_values) = input.get("notes").and_then(|v| v.as_array()) else {
            return Ok(ToolResult::error(
                "notes must be a non-empty array".to_string(),
            ));
        };

        let mut notes: Vec<NoteInput> = Vec::new();
        for nv in note_values {
            let Some(title) = nv
                .get("title")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
            else {
                return Ok(ToolResult::error(
                    "every note needs a non-empty title".to_string(),
                ));
            };
            let note_type = nv
                .get("type")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let observations: Vec<String> = nv
                .get("observations")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();
            let relations: Vec<Value> = nv
                .get("relations")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            if observations.is_empty() && relations.is_empty() {
                return Ok(ToolResult::error(format!(
                    "note \"{title}\" has no observations or relations — give it some content"
                )));
            }

            notes.push(NoteInput {
                title: title.to_string(),
                note_type,
                observations,
                relations,
            });
        }

        if notes.is_empty() {
            return Ok(ToolResult::error(
                "notes must be a non-empty array".to_string(),
            ));
        }

        match review::queue_batch(&self.config, self.pool.clone(), &summary, &notes).await {
            Ok(queued) => Ok(ToolResult::success(format!(
                "Queued {} note(s) as batch {} ({} file(s) changed). \
                 Run /kg to review and approve.",
                queued.notes_written, queued.id, queued.files_changed
            ))),
            Err(e) => Ok(ToolResult::error(format!("kg_remember failed: {e}"))),
        }
    }
}
