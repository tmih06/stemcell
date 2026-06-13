-- Knowledge-graph review queue: one row per parked memory-write batch.
--
-- When the review gate is on, the agent's `kg_remember` tool seals its writes
-- onto a git branch (kg/batch/<id>) in a sibling worktree and parks a row here.
-- The user reviews via `/kg`, then approves (merge to main) or declines (drop
-- branch). The diff itself is NOT stored — it is always reproducible from
-- `branch` against `base_sha`, so this table holds only batch state + cached
-- diff stats for the list view.

CREATE TABLE IF NOT EXISTS kg_pending_batch (
    id            TEXT PRIMARY KEY,                 -- uuid; also the kg/batch/<id> branch suffix
    branch        TEXT NOT NULL,                    -- full branch name, kg/batch/<id>
    base_sha      TEXT NOT NULL,                    -- main HEAD when the branch was cut (diff base)
    summary       TEXT NOT NULL,                    -- agent-supplied one-line description
    status        TEXT NOT NULL DEFAULT 'pending',  -- pending | approved | declined | conflicted
    worktree_path TEXT,                             -- sibling worktree dir; NULL once cleaned up
    merge_sha     TEXT,                             -- merge commit sha (set on approve; enables revert)
    files_changed INTEGER NOT NULL DEFAULT 0,
    insertions    INTEGER NOT NULL DEFAULT 0,
    deletions     INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    resolved_at   INTEGER                           -- set when status leaves 'pending'
);

CREATE INDEX IF NOT EXISTS idx_kg_pending_batch_status ON kg_pending_batch (status);
