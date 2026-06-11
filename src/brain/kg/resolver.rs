//! Pure link + anchor resolution.
//!
//! [`Resolver`] maps a wikilink target (a note name) to a vault-relative path
//! using Obsidian semantics: case-insensitive match against the note title,
//! then the filename stem. The anchor helpers resolve a `#heading` or `^block`
//! reference to a `[start, end)` line range so callers can slice-read just the
//! relevant fragment of a note instead of the whole file.
//!
//! Everything here is I/O-free — anchor helpers operate on an already-loaded
//! note body — so the logic is unit-testable without a filesystem.

use super::parser::{block_id_on_line, parse_heading};
use std::collections::HashMap;

/// Case-insensitive resolver from link target name → vault-relative path.
#[derive(Debug, Default, Clone)]
pub struct Resolver {
    by_title: HashMap<String, String>,
    by_stem: HashMap<String, String>,
}

impl Resolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a resolver from `(path, title)` pairs.
    pub fn from_notes<I>(notes: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let mut r = Self::new();
        for (path, title) in notes {
            r.insert(&path, &title);
        }
        r
    }

    /// Register a note. Later inserts win on key collisions.
    pub fn insert(&mut self, path: &str, title: &str) {
        let title_key = title.trim().to_lowercase();
        if !title_key.is_empty() {
            self.by_title.insert(title_key, path.to_string());
        }
        if let Some(stem) = filename_stem(path) {
            self.by_stem.insert(stem.to_lowercase(), path.to_string());
        }
    }

    /// Resolve a wikilink target name to a path. Title match is preferred over
    /// filename-stem match. Returns `None` for a dangling (ghost) link.
    pub fn resolve(&self, target: &str) -> Option<&str> {
        let key = target.trim().to_lowercase();
        if key.is_empty() {
            return None;
        }
        self.by_title
            .get(&key)
            .or_else(|| self.by_stem.get(&key))
            .map(|s| s.as_str())
    }

    /// Number of distinct notes known to the resolver (by stem).
    pub fn len(&self) -> usize {
        self.by_stem.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_stem.is_empty()
    }
}

/// The `.md`-stripped filename of a vault-relative path.
pub fn filename_stem(path: &str) -> Option<String> {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = file.strip_suffix(".md").unwrap_or(file);
    if stem.is_empty() {
        None
    } else {
        Some(stem.to_string())
    }
}

/// Resolve a `#heading` reference to the `[start, end)` line range covering the
/// heading and its content (up to the next heading of the same or higher level).
pub fn heading_range(content: &str, heading: &str) -> Option<(usize, usize)> {
    let target = heading.trim().to_lowercase();
    let lines: Vec<&str> = content.lines().collect();
    let mut start: Option<usize> = None;
    let mut start_level = 0usize;

    for (i, line) in lines.iter().enumerate() {
        if let Some((level, text)) = parse_heading(line) {
            match start {
                None => {
                    if text.trim().to_lowercase() == target {
                        start = Some(i);
                        start_level = level;
                    }
                }
                Some(s) => {
                    if level <= start_level {
                        return Some((s, i));
                    }
                }
            }
        }
    }
    start.map(|s| (s, lines.len()))
}

/// Resolve a `^block` reference to the `[start, end)` line range covering the
/// block (the contiguous paragraph that ends with the block id).
pub fn block_range(content: &str, block_id: &str) -> Option<(usize, usize)> {
    let needle = block_id.trim();
    let lines: Vec<&str> = content.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if block_id_on_line(line).as_deref() == Some(needle) {
            // Expand upward to the start of the paragraph.
            let mut start = i;
            while start > 0 {
                let prev = lines[start - 1];
                if prev.trim().is_empty() || parse_heading(prev).is_some() {
                    break;
                }
                start -= 1;
            }
            return Some((start, i + 1));
        }
    }
    None
}

/// Resolve either a heading or a block anchor (block takes precedence when both
/// are given) to a line range.
pub fn anchor_range(
    content: &str,
    heading: Option<&str>,
    block_id: Option<&str>,
) -> Option<(usize, usize)> {
    if let Some(block) = block_id {
        return block_range(content, block);
    }
    if let Some(h) = heading {
        return heading_range(content, h);
    }
    None
}

/// Join the lines of `content` in the given `[start, end)` range back into text.
pub fn slice_lines(content: &str, range: (usize, usize)) -> String {
    let (start, end) = range;
    content
        .lines()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect::<Vec<_>>()
        .join("\n")
}
