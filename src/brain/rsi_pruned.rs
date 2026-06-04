//! Pruned Sections Tracker
//!
//! Tracks brain-file sections (## and ### headers) that the user has
//! intentionally deleted, so that `sync_templates()` does not re-add
//! them on the next upstream sync.
//!
//! State is persisted to `~/.opencrabs/rsi/pruned.toml`:
//! ```toml
//! [SOUL.md]
//! pruned = ["## Old Section", "### Old Subsection"]
//! pruned_at = "2026-06-04T10:00:00Z"
//! ```
//!
//! Hook points:
//! - `write_opencrabs_file` tool: when cleanup_intent or dedup_intent
//!   shrinks a protected brain file, diff old vs new to find removed
//!   sections and record them.
//! - `sync_templates()`: before appending a new upstream section,
//!   check if its header is in the pruned list and skip if so.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// In-memory representation of the pruned sidecar.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrunedState {
    /// Map from filename (e.g. "SOUL.md") to list of pruned headers.
    pub pruned: HashMap<String, Vec<String>>,
    /// Map from filename to ISO-8601 timestamp of last prune event.
    pub pruned_at: HashMap<String, String>,
}

impl PrunedState {
    /// Load state from `~/.opencrabs/rsi/pruned.toml`.
    /// Returns default (empty) state if file does not exist or fails to parse.
    pub fn load() -> Self {
        let path = Self::state_path();
        if !path.exists() {
            return Self::default();
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("pruned: failed to read pruned.toml: {e}");
                return Self::default();
            }
        };
        Self::parse(&content)
    }

    /// Parse TOML content into PrunedState.
    /// Simple hand-rolled parser to avoid pulling in a TOML crate just
    /// for this one file. Format is simple enough.
    fn parse(content: &str) -> Self {
        let mut state = Self::default();
        let mut current_file: Option<String> = None;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            // Section header: [SOUL.md]
            if trimmed.starts_with('[') && trimmed.ends_with(']') && !trimmed.contains('=') {
                let name = trimmed[1..trimmed.len() - 1].trim().to_string();
                current_file = Some(name);
                continue;
            }
            if let Some(ref file) = current_file
                && let Some((key, value)) = trimmed.split_once('=')
            {
                let key = key.trim();
                let value = value.trim();
                if key == "pruned_at" {
                    let ts = value.trim_matches('"').to_string();
                    state.pruned_at.insert(file.clone(), ts);
                } else if key == "pruned" {
                    let headers = Self::parse_string_array(value);
                    if !headers.is_empty() {
                        state.pruned.insert(file.clone(), headers);
                    }
                }
            }
        }
        state
    }

    /// Parse a TOML array of strings like `["## Foo", "### Bar"]`.
    fn parse_string_array(value: &str) -> Vec<String> {
        let trimmed = value.trim();
        if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
            return Vec::new();
        }
        let inner = &trimmed[1..trimmed.len() - 1];
        let mut result = Vec::new();
        let mut current = String::new();
        let mut in_string = false;
        let mut escape = false;

        for ch in inner.chars() {
            if escape {
                current.push(ch);
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == '"' {
                if in_string {
                    result.push(current.clone());
                    current.clear();
                    in_string = false;
                } else {
                    in_string = true;
                }
                continue;
            }
            if in_string {
                current.push(ch);
            }
        }
        result
    }

    /// Save state to `~/.opencrabs/rsi/pruned.toml`.
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut content = String::from(
            "# Pruned brain-file sections.\n\
             # Sections listed here will NOT be re-added by sync_templates().\n\
             # Edit manually or use `opencrabs pruned clear` to reset.\n\n",
        );
        let mut files: Vec<&String> = self.pruned.keys().collect();
        files.sort();
        for file in files {
            if let Some(headers) = self.pruned.get(file) {
                if headers.is_empty() {
                    continue;
                }
                content.push_str(&format!("[{file}]\n"));
                content.push_str("pruned = [");
                let escaped: Vec<String> = headers
                    .iter()
                    .map(|h| format!("\"{}\"", h.replace('\\', "\\\\").replace('"', "\\\"")))
                    .collect();
                content.push_str(&escaped.join(", "));
                content.push_str("]\n");
                if let Some(ts) = self.pruned_at.get(file) {
                    content.push_str(&format!("pruned_at = \"{ts}\"\n"));
                }
                content.push('\n');
            }
        }
        std::fs::write(&path, content)
    }

    fn state_path() -> PathBuf {
        crate::config::opencrabs_home().join("rsi/pruned.toml")
    }

    /// Check if a given header is pruned for a given file.
    pub fn is_pruned(&self, filename: &str, header: &str) -> bool {
        self.pruned
            .get(filename)
            .map(|headers| headers.iter().any(|h| h == header))
            .unwrap_or(false)
    }

    /// Get all pruned headers for a given file.
    pub fn pruned_headers(&self, filename: &str) -> HashSet<String> {
        self.pruned
            .get(filename)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect()
    }

    /// Record that a section was pruned from a file.
    /// Merges with existing pruned headers (no duplicates).
    pub fn record_pruned(&mut self, filename: &str, headers: Vec<String>) {
        if headers.is_empty() {
            return;
        }
        let entry = self.pruned.entry(filename.to_string()).or_default();
        for h in headers {
            if !entry.contains(&h) {
                entry.push(h);
            }
        }
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        self.pruned_at.insert(filename.to_string(), now);
    }

    /// Clear all pruned entries for a file (or all files if filename is None).
    pub fn clear(&mut self, filename: Option<&str>) {
        if let Some(f) = filename {
            self.pruned.remove(f);
            self.pruned_at.remove(f);
        } else {
            self.pruned.clear();
            self.pruned_at.clear();
        }
    }
}

/// Diff two versions of a brain file and return the section headers
/// (## and ### lines) that were present in `old` but not in `new`.
pub fn detect_removed_sections(old: &str, new: &str) -> Vec<String> {
    let old_headers: HashSet<String> = crate::brain::rsi_sync::extract_section_headers(old)
        .into_iter()
        .collect();
    let new_headers: HashSet<String> = crate::brain::rsi_sync::extract_section_headers(new)
        .into_iter()
        .collect();
    let mut removed: Vec<String> = old_headers.difference(&new_headers).cloned().collect();
    removed.sort();
    removed
}

/// Filter out pruned sections from a string of new sections to append.
/// Returns the filtered string with pruned section blocks removed.
pub fn filter_pruned_sections(new_sections: &str, state: &PrunedState, filename: &str) -> String {
    let pruned = state.pruned_headers(filename);
    if pruned.is_empty() {
        return new_sections.to_string();
    }

    let mut blocks: Vec<(String, Vec<String>)> = Vec::new();
    let mut current_header = String::new();
    let mut current_content: Vec<String> = Vec::new();

    for line in new_sections.lines() {
        if line.starts_with("## ") || line.starts_with("### ") {
            if !current_header.is_empty() {
                blocks.push((current_header.clone(), current_content.clone()));
            }
            current_header = line.to_string();
            current_content = vec![line.to_string()];
        } else if !current_header.is_empty() {
            current_content.push(line.to_string());
        }
    }
    if !current_header.is_empty() {
        blocks.push((current_header, current_content));
    }

    let kept: Vec<String> = blocks
        .into_iter()
        .filter(|(header, _)| !pruned.contains(header))
        .map(|(_, content)| content.join("\n"))
        .collect();

    if kept.is_empty() {
        String::new()
    } else {
        format!("\n{}\n", kept.join("\n\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty() {
        let state = PrunedState::parse("");
        assert!(state.pruned.is_empty());
        assert!(state.pruned_at.is_empty());
    }

    #[test]
    fn test_parse_basic() {
        let toml = "[SOUL.md]\npruned = [\"## Old Section\", \"### Old Subsection\"]\npruned_at = \"2026-06-04T10:00:00Z\"\n\n[TOOLS.md]\npruned = [\"## Deprecated Docs\"]\n";
        let state = PrunedState::parse(toml);
        assert_eq!(state.pruned.len(), 2);
        assert_eq!(
            state.pruned.get("SOUL.md").unwrap(),
            &vec!["## Old Section", "### Old Subsection"]
        );
        assert_eq!(
            state.pruned_at.get("SOUL.md").unwrap(),
            "2026-06-04T10:00:00Z"
        );
        assert_eq!(
            state.pruned.get("TOOLS.md").unwrap(),
            &vec!["## Deprecated Docs"]
        );
    }

    #[test]
    fn test_parse_escaped_quotes() {
        let toml = "[SOUL.md]\npruned = [\"## Section with \\\"quotes\\\"\"]\n";
        let state = PrunedState::parse(toml);
        let headers = state.pruned.get("SOUL.md").unwrap();
        assert_eq!(headers, &vec!["## Section with \"quotes\""]);
    }

    #[test]
    fn test_is_pruned() {
        let mut state = PrunedState::default();
        state.record_pruned("SOUL.md", vec!["## Old".to_string()]);
        assert!(state.is_pruned("SOUL.md", "## Old"));
        assert!(!state.is_pruned("SOUL.md", "## New"));
        assert!(!state.is_pruned("TOOLS.md", "## Old"));
    }

    #[test]
    fn test_record_pruned_no_duplicates() {
        let mut state = PrunedState::default();
        state.record_pruned("SOUL.md", vec!["## A".to_string(), "## B".to_string()]);
        state.record_pruned("SOUL.md", vec!["## B".to_string(), "## C".to_string()]);
        let headers = state.pruned.get("SOUL.md").unwrap();
        assert_eq!(headers, &vec!["## A", "## B", "## C"]);
    }

    #[test]
    fn test_clear_specific() {
        let mut state = PrunedState::default();
        state.record_pruned("SOUL.md", vec!["## A".to_string()]);
        state.record_pruned("TOOLS.md", vec!["## B".to_string()]);
        state.clear(Some("SOUL.md"));
        assert!(!state.pruned.contains_key("SOUL.md"));
        assert!(state.pruned.contains_key("TOOLS.md"));
    }

    #[test]
    fn test_clear_all() {
        let mut state = PrunedState::default();
        state.record_pruned("SOUL.md", vec!["## A".to_string()]);
        state.record_pruned("TOOLS.md", vec!["## B".to_string()]);
        state.clear(None);
        assert!(state.pruned.is_empty());
    }

    #[test]
    fn test_detect_removed_sections() {
        let old = "# Title\n\n## Keep\ncontent\n\n## Remove Me\nold stuff\n\n### Also Removed\n";
        let new = "# Title\n\n## Keep\ncontent\n\n## New Section\n";
        let removed = detect_removed_sections(old, new);
        assert_eq!(removed, vec!["## Remove Me", "### Also Removed"]);
    }

    #[test]
    fn test_detect_removed_sections_no_changes() {
        let content = "# Title\n\n## Keep\ncontent\n";
        let removed = detect_removed_sections(content, content);
        assert!(removed.is_empty());
    }

    #[test]
    fn test_save_roundtrip() {
        let mut state = PrunedState::default();
        state.record_pruned("SOUL.md", vec!["## Old Section".to_string()]);

        let tmp =
            std::env::temp_dir().join(format!("opencrabs_pruned_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("pruned.toml");

        let mut content = String::from("[SOUL.md]\npruned = [");
        let escaped: Vec<String> = state
            .pruned
            .get("SOUL.md")
            .unwrap()
            .iter()
            .map(|h| format!("\"{}\"", h.replace('"', "\\\"")))
            .collect();
        content.push_str(&escaped.join(", "));
        content.push_str("]\n");
        std::fs::write(&path, &content).unwrap();

        let loaded = PrunedState::parse(&std::fs::read_to_string(&path).unwrap());
        assert_eq!(
            loaded.pruned.get("SOUL.md").unwrap(),
            &vec!["## Old Section"]
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
