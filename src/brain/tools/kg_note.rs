//! `kg_note` — capture or extend a knowledge-graph note.
//!
//! Writes/updates a markdown note in the vault and reindexes it. Updates are
//! *surgical*: new observations/relations are appended to the relevant sections,
//! never a full rewrite, so user-authored content is preserved. New notes get a
//! frontmatter header and are filed into the folder for their type.

use super::error::Result;
use super::r#trait::{Tool, ToolCapability, ToolExecutionContext, ToolResult};
use crate::brain::kg::parser::parse_heading;
use crate::brain::kg::sync;
use crate::brain::kg::vault::{self, Vault};
use crate::db::KnowledgeGraphRepository;
use async_trait::async_trait;
use serde_json::Value;

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
        let mode = input.get("mode").and_then(|v| v.as_str()).unwrap_or("create");

        let observation_bullets: Vec<String> = input
            .get("observations")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| observation_bullet(s))
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
        let rel = match self.repo.get_note_by_ref(&title).await {
            Ok(Some(rec)) => rec.path,
            Ok(None) => format!(
                "{}/{}",
                vault::folder_for_type(note_type.as_deref()),
                vault::slug_filename(&title)
            ),
            Err(e) => return Ok(ToolResult::error(format!("kg_note failed: {e}"))),
        };

        let exists = self.vault.exists(&rel);
        if mode == "append" && !exists {
            return Ok(ToolResult::error(format!(
                "mode=append but no note exists for \"{title}\" — use mode=create"
            )));
        }

        let content = if exists {
            let mut content = self.vault.read_note(&rel).unwrap_or_default();
            content = insert_bullets(&content, "Observations", &observation_bullets);
            content = insert_bullets(&content, "Relations", &relation_bullets);
            content
        } else {
            build_note(&title, note_type.as_deref(), &observation_bullets, &relation_bullets)
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
            "{action} {rel} (+{} observation(s), +{} relation(s))",
            observation_bullets.len(),
            relation_bullets.len()
        )))
    }
}

/// Format an observation string as a bullet, leaving any existing `- ` prefix.
fn observation_bullet(s: &str) -> String {
    let s = s.trim();
    if s.starts_with("- ") {
        s.to_string()
    } else {
        format!("- {s}")
    }
}

/// Format a relation input (object `{type,target}` or bare string) as a bullet.
fn relation_bullet(v: &Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        // A raw string may already contain a [[link]]; otherwise treat it as a
        // bare target (links_to).
        if s.contains("[[") {
            return Some(if s.starts_with("- ") {
                s.to_string()
            } else {
                format!("- {s}")
            });
        }
        return Some(format!("- [[{s}]]"));
    }
    let obj = v.as_object()?;
    let target = obj.get("target").and_then(|t| t.as_str())?.trim();
    if target.is_empty() {
        return None;
    }
    let link = if target.contains("[[") {
        target.to_string()
    } else {
        format!("[[{target}]]")
    };
    let rtype = obj
        .get("type")
        .and_then(|t| t.as_str())
        .map(str::trim)
        .filter(|t| !t.is_empty() && *t != "links_to");
    Some(match rtype {
        Some(t) => format!("- {t} {link}"),
        None => format!("- {link}"),
    })
}

/// Build a fresh note with frontmatter and the given section bullets.
fn build_note(
    title: &str,
    note_type: Option<&str>,
    observations: &[String],
    relations: &[String],
) -> String {
    let today = chrono::Local::now().format("%Y-%m-%d");
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str(&format!("title: {title}\n"));
    s.push_str(&format!("type: {}\n", note_type.unwrap_or("concept")));
    s.push_str(&format!("created: {today}\n"));
    s.push_str("---\n\n");
    s.push_str(&format!("# {title}\n"));
    let mut content = s;
    content = insert_bullets(&content, "Observations", observations);
    content = insert_bullets(&content, "Relations", relations);
    content
}

/// Insert bullets into a `## {section}` section, creating the section at the end
/// if it doesn't exist. Existing content in the section is preserved; new
/// bullets are appended after it.
fn insert_bullets(content: &str, section_title: &str, bullets: &[String]) -> String {
    if bullets.is_empty() {
        return content.to_string();
    }
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();

    let mut section_line = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some((level, text)) = parse_heading(line) {
            if level == 2 && text.eq_ignore_ascii_case(section_title) {
                section_line = Some(i);
                break;
            }
        }
    }

    match section_line {
        Some(h) => {
            // Find the end of the section (next level<=2 heading, else EOF).
            let mut end = lines.len();
            for (j, line) in lines.iter().enumerate().skip(h + 1) {
                if let Some((level, _)) = parse_heading(line) {
                    if level <= 2 {
                        end = j;
                        break;
                    }
                }
            }
            // Trim trailing blank lines inside the section before inserting.
            let mut at = end;
            while at > h + 1 && lines[at - 1].trim().is_empty() {
                at -= 1;
            }
            for (k, b) in bullets.iter().enumerate() {
                lines.insert(at + k, b.clone());
            }
        }
        None => {
            if lines.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
                lines.push(String::new());
            }
            lines.push(format!("## {section_title}"));
            lines.extend(bullets.iter().cloned());
        }
    }

    let mut out = lines.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}
