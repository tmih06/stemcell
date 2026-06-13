//! Note-composition helpers shared by the write tools.
//!
//! Both `kg_note` (direct write) and `kg_remember` (gated batch write) turn a
//! tool's `{observations, relations}` input into Obsidian-compatible markdown and
//! append it to a note surgically — never a full rewrite. These helpers live in
//! the ungated `brain::kg` layer (not behind `tool-kg-note`) so the batch tool and
//! the git-review service can reuse them regardless of which tools are compiled.
//!
//! The bullet/markdown helpers are I/O-free and string-only, so they unit-test
//! without a vault. The one exception is [`resolve_note_rel`], which needs an
//! index lookup to reuse an existing note's path, so it is async over the repo.

use crate::brain::kg::parser::parse_heading;
use crate::brain::kg::vault;
use crate::db::KnowledgeGraphRepository;
use anyhow::Result;
use serde_json::Value;

/// Resolve a note title to its vault-relative path: an existing note (matched by
/// title / filename stem via the index) keeps its current path; a new note is
/// filed under the folder for its type (`concepts/`, `people/`, …). Shared by
/// both write tools so a `kg_note` and a `kg_remember` for the same title target
/// the same file.
pub async fn resolve_note_rel(
    repo: &KnowledgeGraphRepository,
    title: &str,
    note_type: Option<&str>,
) -> Result<String> {
    if let Some(rec) = repo.get_note_by_ref(title).await? {
        return Ok(rec.path);
    }
    Ok(format!(
        "{}/{}",
        vault::folder_for_type(note_type),
        vault::slug_filename(title)
    ))
}

/// Format an observation string as a bullet, leaving any existing `- ` prefix.
pub fn observation_bullet(s: &str) -> String {
    let s = s.trim();
    if s.starts_with("- ") {
        s.to_string()
    } else {
        format!("- {s}")
    }
}

/// Format a relation input (object `{type,target}` or bare string) as a bullet.
pub fn relation_bullet(v: &Value) -> Option<String> {
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

/// Apply observation/relation bullets to a note, returning the new content and
/// the number of observation/relation bullets actually added. `existing` is the
/// note's current markdown when it already exists (bullets are surgically
/// appended and deduped); `None` builds a fresh note. Shared by `kg_note` (direct
/// write) and the git-review batch path so both compose identically.
pub fn compose_content(
    existing: Option<&str>,
    title: &str,
    note_type: Option<&str>,
    observations: &[String],
    relations: &[String],
) -> (String, usize, usize) {
    match existing {
        Some(current) => {
            let (content, added_obs) = insert_bullets(current, "Observations", observations);
            let (content, added_rel) = insert_bullets(&content, "Relations", relations);
            (content, added_obs, added_rel)
        }
        None => (
            build_note(title, note_type, observations, relations),
            observations.len(),
            relations.len(),
        ),
    }
}

/// Build a fresh note with frontmatter and the given section bullets.
pub fn build_note(
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
    (content, _) = insert_bullets(&content, "Observations", observations);
    (content, _) = insert_bullets(&content, "Relations", relations);
    content
}

/// Insert bullets into a `## {section}` section, creating the section at the end
/// if it doesn't exist. Existing content in the section is preserved; new
/// bullets are appended after it. Bullets whose trimmed text already appears in
/// the section are skipped (idempotent append) — this keeps a memory tool from
/// bloating notes with exact-duplicate facts re-remembered across sessions.
///
/// Returns the updated content and the number of bullets actually inserted.
pub fn insert_bullets(content: &str, section_title: &str, bullets: &[String]) -> (String, usize) {
    if bullets.is_empty() {
        return (content.to_string(), 0);
    }
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();

    let mut section_line = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some((level, text)) = parse_heading(line)
            && level == 2
            && text.eq_ignore_ascii_case(section_title)
        {
            section_line = Some(i);
            break;
        }
    }

    match section_line {
        Some(h) => {
            // Find the end of the section (next level<=2 heading, else EOF).
            let mut end = lines.len();
            for (j, line) in lines.iter().enumerate().skip(h + 1) {
                if let Some((level, _)) = parse_heading(line)
                    && level <= 2
                {
                    end = j;
                    break;
                }
            }
            // Dedup against existing section lines only (between the heading and
            // `end`), so identical bullets aren't appended again.
            let existing: std::collections::HashSet<&str> =
                lines[h + 1..end].iter().map(|l| l.trim()).collect();
            let new_bullets: Vec<&String> = bullets
                .iter()
                .filter(|b| !existing.contains(b.trim()))
                .collect();
            if new_bullets.is_empty() {
                // Nothing new — return content unchanged.
                let mut out = lines.join("\n");
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                return (out, 0);
            }
            // Trim trailing blank lines inside the section before inserting.
            let mut at = end;
            while at > h + 1 && lines[at - 1].trim().is_empty() {
                at -= 1;
            }
            let inserted = new_bullets.len();
            for (k, b) in new_bullets.into_iter().enumerate() {
                lines.insert(at + k, b.clone());
            }
            let mut out = lines.join("\n");
            if !out.ends_with('\n') {
                out.push('\n');
            }
            (out, inserted)
        }
        None => {
            if lines.last().map(|l| !l.trim().is_empty()).unwrap_or(false) {
                lines.push(String::new());
            }
            lines.push(format!("## {section_title}"));
            lines.extend(bullets.iter().cloned());
            let mut out = lines.join("\n");
            if !out.ends_with('\n') {
                out.push('\n');
            }
            (out, bullets.len())
        }
    }
}
