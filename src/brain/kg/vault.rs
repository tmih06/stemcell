//! The on-disk vault: a dedicated, Obsidian-openable markdown directory that is
//! the source of truth for the knowledge graph.
//!
//! Default location is `<stemcell_home>/vault` (`~/.stemcell/vault`), overridable
//! via `[memory].vault_dir` in config. A `.obsidian/` marker is scaffolded once
//! so the folder opens cleanly in the Obsidian GUI, alongside the standard
//! `concepts/ people/ projects/ MOCs/ daily/` folders.

use crate::config::Config;
use std::io;
use std::path::{Path, PathBuf};

/// Top-level folders scaffolded in a fresh vault.
pub const FOLDERS: &[&str] = &["concepts", "people", "projects", "MOCs", "daily"];

/// Directories never treated as note content during a walk.
const SKIP_DIRS: &[&str] = &[".obsidian", ".trash", ".git"];

/// How the watcher should react to a raw `notify` event path. See
/// [`Vault::classify_path`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathClass {
    /// A tracked `.md` note at this vault-relative path: index it if it exists
    /// on disk, delete it from the index if it's gone.
    Note(String),
    /// A directory or non-`.md` change under a real folder. Child notes can move
    /// with no per-note event (e.g. a folder rename/delete), so the watcher
    /// falls back to a full reindex to stay correct.
    Other,
    /// Outside the vault, the vault root itself, or under a skipped/hidden
    /// directory (`.obsidian/`, dotdirs). Drop it — no work.
    Ignore,
}

/// A handle to a vault rooted at an absolute directory.
#[derive(Debug, Clone)]
pub struct Vault {
    root: PathBuf,
}

impl Vault {
    /// Open a vault at an explicit root (does not create anything).
    pub fn open(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Resolve the vault directory from config: `[memory].vault_dir` (with `~`
    /// expansion) if set, else `<stemcell_home>/vault`.
    pub fn resolve_dir(config: &Config) -> PathBuf {
        match config
            .memory
            .vault_dir
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(dir) => expand_tilde(dir),
            None => crate::config::stemcell_home().join("vault"),
        }
    }

    /// Open the vault resolved from config.
    pub fn from_config(config: &Config) -> Self {
        Self::open(Self::resolve_dir(config))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create the vault root, the `.obsidian/` marker, and the standard folders
    /// if they do not already exist. Idempotent.
    pub fn ensure_scaffold(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.root)?;

        let obsidian = self.root.join(".obsidian");
        if !obsidian.exists() {
            std::fs::create_dir_all(&obsidian)?;
            // A minimal app.json is enough for Obsidian to treat this as a vault.
            let _ = std::fs::write(obsidian.join("app.json"), "{}\n");
        }

        for folder in FOLDERS {
            std::fs::create_dir_all(self.root.join(folder))?;
        }
        Ok(())
    }

    /// Absolute path for a vault-relative note path.
    pub fn note_path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// Read a note by vault-relative path.
    pub fn read_note(&self, rel: &str) -> io::Result<String> {
        std::fs::read_to_string(self.note_path(rel))
    }

    /// Write a note by vault-relative path, creating parent folders as needed.
    pub fn write_note(&self, rel: &str, content: &str) -> io::Result<()> {
        let path = self.note_path(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)
    }

    /// True if a note exists at the vault-relative path.
    pub fn exists(&self, rel: &str) -> bool {
        self.note_path(rel).exists()
    }

    /// Convert an absolute path under the vault into a `/`-normalized
    /// vault-relative path. Returns `None` if the path is outside the vault.
    pub fn relative(&self, abs: &Path) -> Option<String> {
        let rel = abs.strip_prefix(&self.root).ok()?;
        let joined = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if joined.is_empty() {
            None
        } else {
            Some(joined)
        }
    }

    /// Classify a raw `notify` event path so the watcher knows how to react.
    /// Mirrors the filter [`list_markdown`](Self::list_markdown) applies during a
    /// full walk, which the raw event stream does not.
    ///
    /// The path need not exist on disk: a delete event names a file that is
    /// already gone, so this never touches the filesystem — the caller decides
    /// index-vs-delete by re-checking [`exists`](Self::exists).
    pub fn classify_path(&self, abs: &Path) -> PathClass {
        let Ok(rel) = abs.strip_prefix(&self.root) else {
            return PathClass::Ignore;
        };
        let mut segments: Vec<String> = Vec::new();
        for comp in rel.components() {
            let seg = comp.as_os_str().to_string_lossy();
            // Any dotdir/dotfile or SKIP_DIRS component → ignore (kills the
            // constant `.obsidian/` write noise so it never triggers indexing).
            if seg.starts_with('.') || SKIP_DIRS.contains(&seg.as_ref()) {
                return PathClass::Ignore;
            }
            segments.push(seg.into_owned());
        }
        let joined = segments.join("/");
        if joined.is_empty() {
            // The vault root itself — not actionable on its own.
            PathClass::Ignore
        } else if joined.ends_with(".md") {
            PathClass::Note(joined)
        } else {
            // A directory or non-`.md` file under a real folder. Folder-scope
            // changes (rename/delete of a dir) move child notes with no per-note
            // event, so the watcher falls back to a full reindex to catch them.
            PathClass::Other
        }
    }

    /// All markdown files in the vault as absolute paths (skipping `.obsidian/`,
    /// `.trash/`, `.git/`, and other hidden directories).
    pub fn list_markdown(&self) -> Vec<PathBuf> {
        let mut out = Vec::new();
        collect_markdown(&self.root, &mut out);
        out
    }
}

fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.starts_with('.') || SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            collect_markdown(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
}

/// Sanitize a note title into a safe `.md` filename, preserving spaces (Obsidian
/// allows them) but stripping path separators and reserved characters.
pub fn slug_filename(title: &str) -> String {
    let mut cleaned = String::with_capacity(title.len());
    for c in title.trim().chars() {
        match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => cleaned.push(' '),
            c if c.is_control() => {}
            c => cleaned.push(c),
        }
    }
    let normalized = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let base = if normalized.is_empty() {
        "Untitled".to_string()
    } else {
        normalized
    };
    format!("{base}.md")
}

/// The vault folder a note of a given type belongs in.
pub fn folder_for_type(note_type: Option<&str>) -> &'static str {
    match note_type.map(|t| t.trim().to_lowercase()).as_deref() {
        Some("person" | "people") => "people",
        Some("project" | "projects") => "projects",
        Some("moc" | "hub" | "map") => "MOCs",
        Some("daily" | "journal") => "daily",
        _ => "concepts",
    }
}

/// Infer a note type from the top-level folder of a vault-relative path, for
/// notes whose frontmatter omits an explicit `type`.
pub fn type_from_path(rel: &str) -> Option<String> {
    let top = rel.split('/').next()?;
    match top {
        "concepts" => Some("concept".to_string()),
        "people" => Some("person".to_string()),
        "projects" => Some("project".to_string()),
        "MOCs" => Some("moc".to_string()),
        "daily" => Some("daily".to_string()),
        _ => None,
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
    }
    if path == "~"
        && let Some(home) = dirs::home_dir()
    {
        return home;
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_filename_sanitizes() {
        assert_eq!(slug_filename("Rust Async"), "Rust Async.md");
        assert_eq!(slug_filename("a/b:c?"), "a b c.md");
        assert_eq!(slug_filename("   "), "Untitled.md");
    }

    #[test]
    fn folder_for_type_maps() {
        assert_eq!(folder_for_type(Some("person")), "people");
        assert_eq!(folder_for_type(Some("MOC")), "MOCs");
        assert_eq!(folder_for_type(None), "concepts");
    }

    #[test]
    fn type_from_path_infers() {
        assert_eq!(type_from_path("people/Alice.md").as_deref(), Some("person"));
        assert_eq!(type_from_path("concepts/X.md").as_deref(), Some("concept"));
        assert_eq!(type_from_path("misc/Y.md"), None);
    }
}
