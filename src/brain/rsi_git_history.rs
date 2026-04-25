//! Git-history helpers used by the RSI loop to suppress stale alerts.
//!
//! 2026-04-25: even with the 7-day window, RSI alerts re-fire on tools
//! that broke once inside the window and were fixed by a commit later
//! that same window. The window says "ignore old failures"; this module
//! says "ignore failures that have a fix commit between them and now".
//!
//! The helper shells out to `git log --since=<ts> --grep=<term> -i
//! --no-merges --format=%H%x09%s` in the resolved opencrabs source
//! directory. Returns an empty Vec on any error (no git, not a repo,
//! no source dir resolvable, etc.) — RSI degrades to its window-only
//! behaviour when git context isn't available, never crashes.
//!
//! The grep is case-insensitive on the commit MESSAGE, not on the
//! diff. We rely on the convention that commits about a tool mention
//! the tool name in the subject (`fix(provider): unwrap proxy error`,
//! `fix(browser): name the actual browser`, etc.) — true for ~all
//! recent commits in this repo per `git log --oneline`.
//!
//! Path filtering would be more rigorous but requires a tool→path map
//! which adds maintenance burden when tools move files. Subject grep
//! is robust to file moves and good enough at the alert-suppression
//! signal level.

use std::path::{Path, PathBuf};
use std::process::Command;

/// One commit summary as returned by `commits_matching_since`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSummary {
    pub sha: String,
    pub subject: String,
}

/// Resolve the opencrabs source repo root. Returns `None` when we have
/// no plausible candidate — RSI then skips the git-context check.
///
/// Resolution order:
/// 1. `OPENCRABS_SRC` env var, if set and points to a directory with `.git/`.
/// 2. `std::env::current_dir()`, if it has a `Cargo.toml` AND that toml
///    contains `name = "opencrabs"` (handles dev-checkout case).
///
/// We deliberately don't walk parent directories — too easy to silently
/// pick up the wrong repo (e.g. running opencrabs from inside a
/// monorepo subdir). An explicit env var or running from the project
/// root are the supported configurations.
pub fn resolve_source_repo() -> Option<PathBuf> {
    if let Ok(env_dir) = std::env::var("OPENCRABS_SRC") {
        let p = PathBuf::from(env_dir);
        if p.join(".git").exists() {
            return Some(p);
        }
    }
    let cwd = std::env::current_dir().ok()?;
    let cargo = cwd.join("Cargo.toml");
    if cargo.exists()
        && let Ok(text) = std::fs::read_to_string(&cargo)
        && text.contains(r#"name = "opencrabs""#)
        && cwd.join(".git").exists()
    {
        return Some(cwd);
    }
    None
}

/// Run `git log` and return commits newer than `since_iso` whose
/// subject (case-insensitive) matches `term`. Empty Vec on any error.
///
/// Both arguments are user-provided in the sense that the caller picks
/// them; we still pass them through `git log` argv unchanged. `git log`
/// itself rejects malformed --since values, so we don't validate
/// upstream — its error surfaces as "git command failed: ..." in our
/// trace and we return an empty Vec, leaving the RSI alert intact.
pub fn commits_matching_since(repo: &Path, since_iso: &str, term: &str) -> Vec<CommitSummary> {
    let output = match Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("log")
        .arg(format!("--since={}", since_iso))
        .arg("--grep")
        .arg(term)
        .arg("-i")
        .arg("--no-merges")
        .arg("--format=%H%x09%s")
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            tracing::debug!(
                "rsi_git_history: git log failed in {}: {}",
                repo.display(),
                e
            );
            return Vec::new();
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::debug!(
            "rsi_git_history: git log non-zero in {}: status={:?} stderr={}",
            repo.display(),
            output.status.code(),
            stderr.trim()
        );
        return Vec::new();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_git_log_output(&stdout)
}

/// Pure parser for `git log --format=%H%x09%s` output. Extracted so
/// unit tests can feed fixtures without spawning git.
pub(crate) fn parse_git_log_output(stdout: &str) -> Vec<CommitSummary> {
    stdout
        .lines()
        .filter_map(|line| {
            let (sha, subject) = line.split_once('\t')?;
            let sha = sha.trim();
            let subject = subject.trim();
            if sha.is_empty() || subject.is_empty() {
                return None;
            }
            Some(CommitSummary {
                sha: sha.to_string(),
                subject: subject.to_string(),
            })
        })
        .collect()
}
