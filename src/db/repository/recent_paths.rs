//! Recent Paths Repository
//!
//! Stores paths the agent has successfully accessed (read/edit/write/ls/grep)
//! keyed by `working_directory`. The point is cross-session continuity: if a
//! prior session figured out that `lib/presentation/foo_screen/foo_screen.dart`
//! is real, a new session on the same project should not have to rediscover
//! it via guess-and-fail. The agent service reads this at every prompt build
//! and surfaces a small "Recently accessed" anchor section in the system
//! prompt.
//!
//! Both `working_directory` and `path` are stored in `~/...` collapsed form
//! so the same project on different machines (or under different OS user
//! names) hits the same key.

use crate::db::Pool;
use crate::db::database::interact_err;
use anyhow::{Context, Result};
use rusqlite::params;

#[derive(Clone)]
pub struct RecentPathsRepository {
    pool: Pool,
}

impl RecentPathsRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Record that `path` was just successfully accessed under
    /// `working_directory`. Both strings should already be collapsed
    /// to `~/...` form by the caller. Updates `last_accessed` on
    /// repeat hits so the LRU ordering stays correct.
    pub async fn record(&self, working_directory: &str, path: &str) -> Result<()> {
        let wd = working_directory.to_string();
        let p = path.to_string();
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| {
                conn.execute(
                    "INSERT INTO recent_paths (working_directory, path, last_accessed) \
                     VALUES (?1, ?2, strftime('%s', 'now')) \
                     ON CONFLICT(working_directory, path) DO UPDATE \
                     SET last_accessed = strftime('%s', 'now')",
                    params![wd, p],
                )
            })
            .await
            .map_err(interact_err)?
            .context("Failed to record recent path")?;
        Ok(())
    }

    /// Top `limit` most-recently accessed paths for a given working
    /// directory, most-recent first. Returns an empty Vec when the
    /// directory has no recorded paths yet.
    pub async fn top_for_dir(&self, working_directory: &str, limit: usize) -> Result<Vec<String>> {
        let wd = working_directory.to_string();
        let limit = limit as i64;
        self.pool
            .get()
            .await
            .context("Failed to get connection")?
            .interact(move |conn| {
                let mut stmt = conn.prepare_cached(
                    "SELECT path FROM recent_paths \
                     WHERE working_directory = ?1 \
                     ORDER BY last_accessed DESC \
                     LIMIT ?2",
                )?;
                let rows = stmt.query_map(params![wd, limit], |row| row.get::<_, String>(0))?;
                rows.collect::<std::result::Result<Vec<_>, _>>()
            })
            .await
            .map_err(interact_err)?
            .context("Failed to query recent paths")
    }
}
