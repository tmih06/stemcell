//! Obsidian-style knowledge-graph long-term memory.
//!
//! A dedicated, Obsidian-openable markdown vault (default `~/.stemcell/vault/`)
//! is the durable substrate for the agent's long-term knowledge: atomic notes
//! linked by `[[wikilinks]]`, with typed observations and relations. Files on
//! disk are the source of truth; the SQLite [`KnowledgeGraphRepository`] is a
//! rebuildable index over them.
//!
//! Layers:
//! - [`parser`] — pure markdown → structured note (frontmatter, links,
//!   observations, relations, anchors).
//! - [`resolver`] — pure link/anchor resolution (name → path, anchor → line
//!   range).
//! - `vault` / `sync` / `traverse` — filesystem, indexing, and bounded graph
//!   walks (added in later build chunks).
//!
//! [`KnowledgeGraphRepository`]: crate::db::KnowledgeGraphRepository

pub mod compose;
pub mod git_review;
pub mod parser;
pub mod resolver;
pub mod review;
pub mod sync;
pub mod traverse;
pub mod vault;
