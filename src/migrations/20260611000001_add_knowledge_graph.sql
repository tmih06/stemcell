-- Obsidian-style knowledge-graph long-term memory.
--
-- Files on disk (the vault at ~/.stemcell/vault/) are the source of truth;
-- these tables are a rebuildable index over them. One `kg_note` row per
-- markdown file; `kg_observation` and `kg_relation` are the atomic facts and
-- typed edges parsed out of each note. `kg_relation.to_id` is nullable so
-- unresolved ("dangling"/ghost) wikilinks are first-class rows — resolution
-- back-fills `to_id` once the target note exists.
--
-- NOTE: production connections do NOT enable `PRAGMA foreign_keys` (only the
-- in-memory test pool does), so the ON DELETE clauses below are advisory. The
-- KnowledgeGraphRepository deletes child rows explicitly so cleanup behaves
-- identically with FK enforcement on or off.

CREATE TABLE IF NOT EXISTS kg_note (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    path             TEXT NOT NULL UNIQUE,          -- vault-relative, '/'-separated, ends in .md
    title            TEXT NOT NULL,
    note_type        TEXT,                          -- concept | person | project | moc | daily | ...
    frontmatter_json TEXT,                          -- raw YAML frontmatter as JSON (cheap fact extraction)
    checksum         TEXT NOT NULL,                 -- sha256 of file bytes (sync change-detection)
    mtime            INTEGER NOT NULL DEFAULT 0,    -- file mtime, unix seconds
    size             INTEGER NOT NULL DEFAULT 0,    -- file size in bytes
    created_at       INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at       INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_kg_note_title ON kg_note (title);
CREATE INDEX IF NOT EXISTS idx_kg_note_type  ON kg_note (note_type);

-- FTS5 index over title + body + observations, keyed by note_id (UNINDEXED so
-- it round-trips back to kg_note.id). Writes are managed explicitly by the
-- repository (delete-by-note_id then insert) rather than external-content
-- triggers, so the index stays correct regardless of FK pragma state.
CREATE VIRTUAL TABLE IF NOT EXISTS kg_note_fts USING fts5 (
    note_id UNINDEXED,
    title,
    body,
    observations,
    tokenize = 'porter unicode61'
);

CREATE TABLE IF NOT EXISTS kg_observation (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    note_id   INTEGER NOT NULL REFERENCES kg_note (id) ON DELETE CASCADE,
    category  TEXT,           -- bullet category, e.g. fact | gotcha | decision (nullable)
    content   TEXT NOT NULL,
    tags_json TEXT,           -- inline #tags as a JSON array
    context   TEXT            -- trailing "(parenthetical context)" if present
);

CREATE INDEX IF NOT EXISTS idx_kg_observation_note ON kg_observation (note_id);

CREATE TABLE IF NOT EXISTS kg_relation (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    from_id       INTEGER NOT NULL REFERENCES kg_note (id) ON DELETE CASCADE,
    to_id         INTEGER REFERENCES kg_note (id) ON DELETE SET NULL,  -- NULL = dangling/ghost link
    to_name       TEXT NOT NULL,                                       -- always set: the raw link target name
    relation_type TEXT NOT NULL,                                       -- links_to | depends_on | contrasts_with | ...
    context       TEXT
);

CREATE INDEX IF NOT EXISTS idx_kg_relation_from    ON kg_relation (from_id);
CREATE INDEX IF NOT EXISTS idx_kg_relation_to      ON kg_relation (to_id);
CREATE INDEX IF NOT EXISTS idx_kg_relation_to_name ON kg_relation (to_name);
CREATE INDEX IF NOT EXISTS idx_kg_relation_type    ON kg_relation (relation_type);
