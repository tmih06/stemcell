//! Knowledge Graph Repository
//!
//! Persists the rebuildable index over the Obsidian-style markdown vault at
//! `~/.stemcell/vault/`. Files on disk are the source of truth; these tables
//! are derived from them by the `brain::kg::sync` indexer.
//!
//! The schema (see `20260611000001_add_knowledge_graph.sql`) is four tables:
//! `kg_note` (one row per file), `kg_note_fts` (FTS5 over title/body/
//! observations), `kg_observation` (atomic typed facts), and `kg_relation`
//! (typed edges; `to_id` nullable so unresolved "ghost" links are first-class
//! rows that resolution back-fills).
//!
//! ## Why explicit child deletes
//!
//! Production connections do not enable `PRAGMA foreign_keys`, so the
//! `ON DELETE CASCADE`/`SET NULL` clauses in the schema only fire in the
//! in-memory test pool. Every method here that removes a note also removes its
//! observations, outgoing relations, and FTS row by hand, and nulls incoming
//! relations back to ghost state — so behaviour is identical with FK on or off.

use crate::db::Pool;
use crate::db::database::interact_err;
use anyhow::{Context, Result};
use rusqlite::{Connection, params, params_from_iter};

/// Direction of a relation query relative to the queried note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkDirection {
    /// Edges where the queried note is the source (`from_id`).
    Out,
    /// Edges where the queried note is the target (`to_id`) — i.e. backlinks.
    In,
    /// Both directions.
    Both,
}

/// Everything needed to upsert a note row plus its FTS body. `body` is only
/// used to (re)build the FTS index; it is not stored verbatim in `kg_note`.
#[derive(Debug, Clone)]
pub struct NoteUpsert {
    pub path: String,
    pub title: String,
    pub note_type: Option<String>,
    pub frontmatter_json: Option<String>,
    pub body: String,
    pub checksum: String,
    pub mtime: i64,
    pub size: i64,
}

/// A typed observation bullet parsed from a note's `## Observations` section.
#[derive(Debug, Clone)]
pub struct ObservationInput {
    pub category: Option<String>,
    pub content: String,
    pub tags_json: Option<String>,
    pub context: Option<String>,
}

/// A typed relation (edge) parsed from a note's `## Relations` section.
#[derive(Debug, Clone)]
pub struct RelationInput {
    pub to_name: String,
    pub relation_type: String,
    pub context: Option<String>,
}

/// A stored note row (without body).
#[derive(Debug, Clone)]
pub struct NoteRecord {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub note_type: Option<String>,
    pub frontmatter_json: Option<String>,
    pub checksum: String,
    pub mtime: i64,
    pub size: i64,
}

/// A stored observation row.
#[derive(Debug, Clone)]
pub struct ObservationRecord {
    pub category: Option<String>,
    pub content: String,
    pub tags_json: Option<String>,
    pub context: Option<String>,
}

/// An FTS entry-point hit, ranked by bm25.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub note_id: i64,
    pub path: String,
    pub title: String,
    pub note_type: Option<String>,
    pub snippet: String,
}

/// A neighbour reached from a note via one relation edge.
#[derive(Debug, Clone)]
pub struct Neighbor {
    /// True if this edge goes *out* of the queried note, false if it's a backlink.
    pub outgoing: bool,
    pub relation_type: String,
    /// The other end's id (None for an outgoing ghost link).
    pub other_id: Option<i64>,
    /// The other end's name — `to_name` for outgoing, the source title for incoming.
    pub other_name: String,
    pub other_title: Option<String>,
    pub other_path: Option<String>,
}

#[derive(Clone)]
pub struct KnowledgeGraphRepository {
    pool: Pool,
}

impl KnowledgeGraphRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Upsert a note and fully replace its observations, relations, and FTS row
    /// in a single transaction. Outgoing relations are inserted with
    /// `to_id = NULL`; call [`resolve_dangling_links`](Self::resolve_dangling_links)
    /// afterwards (once per sync pass) to back-fill resolved targets. Incoming
    /// relations to this note are preserved.
    ///
    /// Returns the note's row id.
    pub async fn index_note(
        &self,
        note: NoteUpsert,
        observations: Vec<ObservationInput>,
        relations: Vec<RelationInput>,
    ) -> Result<i64> {
        let observations_text = observations
            .iter()
            .map(|o| o.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| -> rusqlite::Result<i64> {
                let tx = conn.transaction()?;

                let note_id: i64 = tx.query_row(
                    "INSERT INTO kg_note \
                       (path, title, note_type, frontmatter_json, checksum, mtime, size, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, strftime('%s','now'), strftime('%s','now')) \
                     ON CONFLICT(path) DO UPDATE SET \
                       title = excluded.title, \
                       note_type = excluded.note_type, \
                       frontmatter_json = excluded.frontmatter_json, \
                       checksum = excluded.checksum, \
                       mtime = excluded.mtime, \
                       size = excluded.size, \
                       updated_at = strftime('%s','now') \
                     RETURNING id",
                    params![
                        note.path,
                        note.title,
                        note.note_type,
                        note.frontmatter_json,
                        note.checksum,
                        note.mtime,
                        note.size,
                    ],
                    |row| row.get(0),
                )?;

                // Replace children + FTS for this note (incoming relations untouched).
                tx.execute(
                    "DELETE FROM kg_observation WHERE note_id = ?1",
                    params![note_id],
                )?;
                tx.execute(
                    "DELETE FROM kg_relation WHERE from_id = ?1",
                    params![note_id],
                )?;
                tx.execute(
                    "DELETE FROM kg_note_fts WHERE note_id = ?1",
                    params![note_id],
                )?;

                {
                    let mut obs_stmt = tx.prepare_cached(
                        "INSERT INTO kg_observation (note_id, category, content, tags_json, context) \
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                    )?;
                    for o in &observations {
                        obs_stmt.execute(params![
                            note_id,
                            o.category,
                            o.content,
                            o.tags_json,
                            o.context
                        ])?;
                    }

                    let mut rel_stmt = tx.prepare_cached(
                        "INSERT INTO kg_relation (from_id, to_id, to_name, relation_type, context) \
                         VALUES (?1, NULL, ?2, ?3, ?4)",
                    )?;
                    for r in &relations {
                        rel_stmt.execute(params![
                            note_id,
                            r.to_name,
                            r.relation_type,
                            r.context
                        ])?;
                    }

                    tx.prepare_cached(
                        "INSERT INTO kg_note_fts (note_id, title, body, observations) \
                         VALUES (?1, ?2, ?3, ?4)",
                    )?
                    .execute(params![note_id, note.title, note.body, observations_text])?;
                }

                tx.commit()?;
                Ok(note_id)
            })
            .await
            .map_err(interact_err)?
            .context("Failed to index knowledge-graph note")
    }

    /// Look up a note by its exact vault-relative path.
    pub async fn get_note_by_path(&self, path: &str) -> Result<Option<NoteRecord>> {
        let path = path.to_string();
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| -> rusqlite::Result<Option<NoteRecord>> {
                let mut stmt = conn.prepare_cached(
                    "SELECT id, path, title, note_type, frontmatter_json, checksum, mtime, size \
                     FROM kg_note WHERE path = ?1",
                )?;
                stmt.query_row(params![path], note_record_from_row)
                    .map(Some)
                    .or_else(|e| match e {
                        rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        other => Err(other),
                    })
            })
            .await
            .map_err(interact_err)?
            .context("Failed to query note by path")
    }

    /// Look up a note by id.
    pub async fn get_note_by_id(&self, id: i64) -> Result<Option<NoteRecord>> {
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| -> rusqlite::Result<Option<NoteRecord>> {
                let mut stmt = conn.prepare_cached(
                    "SELECT id, path, title, note_type, frontmatter_json, checksum, mtime, size \
                     FROM kg_note WHERE id = ?1",
                )?;
                stmt.query_row(params![id], note_record_from_row)
                    .map(Some)
                    .or_else(|e| match e {
                        rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        other => Err(other),
                    })
            })
            .await
            .map_err(interact_err)?
            .context("Failed to query note by id")
    }

    /// Resolve a wikilink target name to a note using Obsidian semantics:
    /// case-insensitive match against the note title, then the filename stem
    /// (top-level or nested). Returns the first match (title preferred).
    pub async fn get_note_by_name(&self, name: &str) -> Result<Option<NoteRecord>> {
        let name_lc = name.trim().to_lowercase();
        let exact_md = format!("{name_lc}.md");
        let like_pattern = format!("%/{}.md", escape_like(&name_lc));
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| -> rusqlite::Result<Option<NoteRecord>> {
                let mut stmt = conn.prepare_cached(
                    "SELECT id, path, title, note_type, frontmatter_json, checksum, mtime, size \
                     FROM kg_note \
                     WHERE lower(title) = ?1 \
                        OR lower(path) = ?2 \
                        OR lower(path) LIKE ?3 ESCAPE '\\' \
                     ORDER BY (lower(title) = ?1) DESC \
                     LIMIT 1",
                )?;
                stmt.query_row(
                    params![name_lc, exact_md, like_pattern],
                    note_record_from_row,
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })
            })
            .await
            .map_err(interact_err)?
            .context("Failed to query note by name")
    }

    /// Resolve a note reference that may be an exact vault-relative path
    /// (ending in `.md`) or a wikilink-style name (title / filename stem).
    pub async fn get_note_by_ref(&self, reference: &str) -> Result<Option<NoteRecord>> {
        let r = reference.trim();
        if r.ends_with(".md")
            && let Some(note) = self.get_note_by_path(r).await?
        {
            return Ok(Some(note));
        }
        self.get_note_by_name(r).await
    }

    /// FTS5 entry-point search ranked by bm25. Returns title/path/type plus a
    /// short one-line body snippet — never full bodies.
    pub async fn search_fts(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let Some(match_query) = fts_match_query(query) else {
            return Ok(Vec::new());
        };
        let limit = limit.max(1) as i64;
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| -> rusqlite::Result<Vec<SearchHit>> {
                let mut stmt = conn.prepare_cached(
                    "SELECT n.id, n.path, n.title, n.note_type, \
                            snippet(kg_note_fts, 2, '', '', '…', 12) \
                     FROM kg_note_fts f \
                     JOIN kg_note n ON n.id = f.note_id \
                     WHERE kg_note_fts MATCH ?1 \
                     ORDER BY bm25(kg_note_fts) \
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![match_query, limit], |row| {
                    let title: String = row.get(2)?;
                    let mut snippet: String = row.get(4)?;
                    if snippet.trim().is_empty() {
                        snippet = title.clone();
                    }
                    Ok(SearchHit {
                        note_id: row.get(0)?,
                        path: row.get(1)?,
                        title,
                        note_type: row.get(3)?,
                        snippet,
                    })
                })?;
                rows.collect()
            })
            .await
            .map_err(interact_err)?
            .context("Failed to run knowledge-graph FTS search")
    }

    /// Relations adjacent to a note in the requested direction.
    pub async fn neighbors(&self, note_id: i64, direction: LinkDirection) -> Result<Vec<Neighbor>> {
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| -> rusqlite::Result<Vec<Neighbor>> {
                let mut out = Vec::new();
                if matches!(direction, LinkDirection::Out | LinkDirection::Both) {
                    collect_outgoing(conn, note_id, &mut out)?;
                }
                if matches!(direction, LinkDirection::In | LinkDirection::Both) {
                    collect_incoming(conn, note_id, &mut out)?;
                }
                Ok(out)
            })
            .await
            .map_err(interact_err)?
            .context("Failed to query note neighbors")
    }

    /// Backlinks: notes whose relations point at `note_id`.
    pub async fn backlinks(&self, note_id: i64) -> Result<Vec<Neighbor>> {
        self.neighbors(note_id, LinkDirection::In).await
    }

    /// Observations attached to a note, in insertion order.
    pub async fn observations_for_note(&self, note_id: i64) -> Result<Vec<ObservationRecord>> {
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| -> rusqlite::Result<Vec<ObservationRecord>> {
                let mut stmt = conn.prepare_cached(
                    "SELECT category, content, tags_json, context \
                     FROM kg_observation WHERE note_id = ?1 ORDER BY id",
                )?;
                let rows = stmt.query_map(params![note_id], |row| {
                    Ok(ObservationRecord {
                        category: row.get(0)?,
                        content: row.get(1)?,
                        tags_json: row.get(2)?,
                        context: row.get(3)?,
                    })
                })?;
                rows.collect()
            })
            .await
            .map_err(interact_err)?
            .context("Failed to query note observations")
    }

    /// Total degree (outgoing + incoming edges) of a note — a cheap
    /// degree-centrality signal for traversal ranking.
    pub async fn degree(&self, note_id: i64) -> Result<i64> {
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| -> rusqlite::Result<i64> {
                conn.query_row(
                    "SELECT \
                       (SELECT COUNT(*) FROM kg_relation WHERE from_id = ?1) + \
                       (SELECT COUNT(*) FROM kg_relation WHERE to_id = ?1)",
                    params![note_id],
                    |row| row.get(0),
                )
            })
            .await
            .map_err(interact_err)?
            .context("Failed to compute note degree")
    }

    /// Back-fill `to_id` for every dangling relation whose `to_name` now
    /// resolves to an existing note. Returns the number of rows resolved.
    ///
    /// Resolves each distinct dangling name with a plain lookup (SQLite does
    /// not allow a correlated subquery referencing the `UPDATE` target table),
    /// then updates the matching ghost rows.
    pub async fn resolve_dangling_links(&self) -> Result<usize> {
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| -> rusqlite::Result<usize> {
                let tx = conn.transaction()?;

                let names: Vec<String> = {
                    let mut stmt =
                        tx.prepare("SELECT DISTINCT to_name FROM kg_relation WHERE to_id IS NULL")?;
                    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
                    rows.collect::<rusqlite::Result<Vec<_>>>()?
                };

                let mut resolved = 0usize;
                {
                    let mut find = tx.prepare_cached(
                        "SELECT id FROM kg_note \
                         WHERE lower(title) = ?1 \
                            OR lower(path) = ?2 \
                            OR lower(path) LIKE ?3 ESCAPE '\\' \
                         ORDER BY (lower(title) = ?1) DESC \
                         LIMIT 1",
                    )?;
                    let mut upd = tx.prepare_cached(
                        "UPDATE kg_relation SET to_id = ?1 WHERE to_id IS NULL AND to_name = ?2",
                    )?;
                    for name in &names {
                        let name_lc = name.trim().to_lowercase();
                        let exact_md = format!("{name_lc}.md");
                        let like_pattern = format!("%/{}.md", escape_like(&name_lc));
                        let target: Option<i64> = find
                            .query_row(params![name_lc, exact_md, like_pattern], |r| r.get(0))
                            .map(Some)
                            .or_else(|e| match e {
                                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                                other => Err(other),
                            })?;
                        if let Some(id) = target {
                            resolved += upd.execute(params![id, name])?;
                        }
                    }
                }

                tx.commit()?;
                Ok(resolved)
            })
            .await
            .map_err(interact_err)?
            .context("Failed to resolve dangling links")
    }

    /// All `(path, checksum)` pairs currently indexed — used by the sync pass
    /// to skip unchanged files and detect deletions.
    pub async fn all_paths_with_checksums(&self) -> Result<Vec<(String, String)>> {
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| -> rusqlite::Result<Vec<(String, String)>> {
                let mut stmt = conn.prepare_cached("SELECT path, checksum FROM kg_note")?;
                let rows = stmt.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                rows.collect()
            })
            .await
            .map_err(interact_err)?
            .context("Failed to list indexed notes")
    }

    /// Number of indexed notes.
    pub async fn note_count(&self) -> Result<i64> {
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| -> rusqlite::Result<i64> {
                conn.query_row("SELECT COUNT(*) FROM kg_note", [], |row| row.get(0))
            })
            .await
            .map_err(interact_err)?
            .context("Failed to count notes")
    }

    /// Delete a single note (and its children) by path. Incoming relations are
    /// reverted to ghost state. Returns true if a row was removed.
    pub async fn delete_note_by_path(&self, path: &str) -> Result<bool> {
        let path = path.to_string();
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| -> rusqlite::Result<bool> {
                let tx = conn.transaction()?;
                let id: Option<i64> = tx
                    .query_row(
                        "SELECT id FROM kg_note WHERE path = ?1",
                        params![path],
                        |r| r.get(0),
                    )
                    .map(Some)
                    .or_else(|e| match e {
                        rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        other => Err(other),
                    })?;
                let removed = match id {
                    Some(id) => {
                        delete_note_cascade(&tx, id)?;
                        true
                    }
                    None => false,
                };
                tx.commit()?;
                Ok(removed)
            })
            .await
            .map_err(interact_err)?
            .context("Failed to delete note by path")
    }

    /// Remove every note whose path is not in `existing_paths` (plus its
    /// children + FTS rows). Incoming relations to pruned notes revert to
    /// ghost state. Returns the number of notes pruned.
    pub async fn prune_missing(&self, existing_paths: &[String]) -> Result<usize> {
        let keep: Vec<String> = existing_paths.to_vec();
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| -> rusqlite::Result<usize> {
                let tx = conn.transaction()?;

                // Collect ids to delete (everything not in `keep`).
                let doomed: Vec<i64> = if keep.is_empty() {
                    let mut stmt = tx.prepare("SELECT id FROM kg_note")?;
                    let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
                    rows.collect::<rusqlite::Result<Vec<_>>>()?
                } else {
                    let placeholders = vec!["?"; keep.len()].join(",");
                    let sql = format!("SELECT id FROM kg_note WHERE path NOT IN ({placeholders})");
                    let mut stmt = tx.prepare(&sql)?;
                    let rows =
                        stmt.query_map(params_from_iter(keep.iter()), |r| r.get::<_, i64>(0))?;
                    rows.collect::<rusqlite::Result<Vec<_>>>()?
                };

                for id in &doomed {
                    delete_note_cascade(&tx, *id)?;
                }

                tx.commit()?;
                Ok(doomed.len())
            })
            .await
            .map_err(interact_err)?
            .context("Failed to prune missing notes")
    }
}

// --- row helpers (run inside an interact closure) ---

fn note_record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteRecord> {
    Ok(NoteRecord {
        id: row.get(0)?,
        path: row.get(1)?,
        title: row.get(2)?,
        note_type: row.get(3)?,
        frontmatter_json: row.get(4)?,
        checksum: row.get(5)?,
        mtime: row.get(6)?,
        size: row.get(7)?,
    })
}

fn collect_outgoing(
    conn: &Connection,
    note_id: i64,
    out: &mut Vec<Neighbor>,
) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.relation_type, r.to_id, r.to_name, n.title, n.path \
         FROM kg_relation r LEFT JOIN kg_note n ON n.id = r.to_id \
         WHERE r.from_id = ?1 ORDER BY r.id",
    )?;
    let rows = stmt.query_map(params![note_id], |row| {
        Ok(Neighbor {
            outgoing: true,
            relation_type: row.get(0)?,
            other_id: row.get(1)?,
            other_name: row.get(2)?,
            other_title: row.get(3)?,
            other_path: row.get(4)?,
        })
    })?;
    for n in rows {
        out.push(n?);
    }
    Ok(())
}

fn collect_incoming(
    conn: &Connection,
    note_id: i64,
    out: &mut Vec<Neighbor>,
) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare_cached(
        "SELECT r.relation_type, r.from_id, src.title, src.path \
         FROM kg_relation r JOIN kg_note src ON src.id = r.from_id \
         WHERE r.to_id = ?1 ORDER BY r.id",
    )?;
    let rows = stmt.query_map(params![note_id], |row| {
        let src_title: String = row.get(2)?;
        Ok(Neighbor {
            outgoing: false,
            relation_type: row.get(0)?,
            other_id: row.get(1)?,
            other_name: src_title.clone(),
            other_title: Some(src_title),
            other_path: row.get(3)?,
        })
    })?;
    for n in rows {
        out.push(n?);
    }
    Ok(())
}

/// Remove a note and all rows that depend on it, reverting incoming edges to
/// ghost state. Caller supplies an open transaction.
fn delete_note_cascade(tx: &rusqlite::Transaction<'_>, id: i64) -> rusqlite::Result<()> {
    tx.execute(
        "UPDATE kg_relation SET to_id = NULL WHERE to_id = ?1",
        params![id],
    )?;
    tx.execute("DELETE FROM kg_relation WHERE from_id = ?1", params![id])?;
    tx.execute("DELETE FROM kg_observation WHERE note_id = ?1", params![id])?;
    tx.execute("DELETE FROM kg_note_fts WHERE note_id = ?1", params![id])?;
    tx.execute("DELETE FROM kg_note WHERE id = ?1", params![id])?;
    Ok(())
}

/// Escape `%`, `_`, and `\` so a user/link string can be embedded literally in
/// a `LIKE ... ESCAPE '\'` pattern.
fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '%' => out.push_str("\\%"),
            '_' => out.push_str("\\_"),
            other => out.push(other),
        }
    }
    out
}

/// Turn a free-text query into a safe FTS5 MATCH expression: lowercased,
/// alphanumeric tokens each double-quoted and AND-joined. Returns `None` when
/// the query has no usable tokens (caller should return no results).
fn fts_match_query(raw: &str) -> Option<String> {
    let mut tokens: Vec<String> = Vec::new();
    for tok in raw.split(|c: char| !c.is_alphanumeric() && c != '_') {
        let t = tok.trim();
        if t.is_empty() {
            continue;
        }
        let escaped = t.replace('"', "\"\"");
        tokens.push(format!("\"{escaped}\""));
    }
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_query_tokenizes_and_quotes() {
        assert_eq!(
            fts_match_query("rust async"),
            Some("\"rust\" \"async\"".into())
        );
        assert_eq!(fts_match_query("  "), None);
        assert_eq!(fts_match_query("a-b.c"), Some("\"a\" \"b\" \"c\"".into()));
    }

    #[test]
    fn escape_like_escapes_wildcards() {
        assert_eq!(escape_like("a_b%c"), "a\\_b\\%c");
        assert_eq!(escape_like("plain"), "plain");
    }
}
