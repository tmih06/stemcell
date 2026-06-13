//! Resolve the current git branch for a working directory.
//!
//! Walks up parent directories looking for `.git/HEAD` (regular repo)
//! or `.git` as a file (worktree / submodule — points at the real
//! gitdir via `gitdir: <path>`). Parses the HEAD content as either:
//! - `ref: refs/heads/<name>` — returns `<name>` (the common branch case)
//! - `ref: refs/<other>/<name>` — returns the last path component
//! - a raw 40-char SHA — returns the first 7 chars (detached HEAD)
//!
//! Used by the TUI status bar to show `~/srv/rs/stemcell (main)`. Called
//! once per render frame, so the actual disk read is throttled by a small
//! per-path TTL cache (see [`current_branch`]) — every keystroke forces a
//! frame, and walking parent dirs to stat + read `.git/HEAD` on each one
//! was a measurable source of input lag on slow/network filesystems.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long a resolved branch is reused before re-reading `.git/HEAD`.
/// Short enough that a `git checkout` in another terminal shows up almost
/// immediately, long enough that holding an arrow key (which fires a frame
/// per keypress) doesn't hammer the filesystem.
const BRANCH_CACHE_TTL: Duration = Duration::from_millis(750);

thread_local! {
    /// Per-path cache of the last resolved branch and when it was read.
    /// Thread-local because the TUI render loop is single-threaded, so this
    /// stays lock-free. Keyed by `cwd` since the status bar and sessions
    /// list resolve branches for different directories.
    static BRANCH_CACHE: RefCell<HashMap<PathBuf, (Instant, Option<String>)>> =
        RefCell::new(HashMap::new());
}

/// Resolve the current git branch (or short detached SHA) for `cwd`,
/// or `None` when the path isn't inside a git repository or HEAD is
/// unreadable / malformed.
///
/// Cached per `cwd` with a [`BRANCH_CACHE_TTL`] freshness window so the
/// per-frame caller doesn't read `.git/HEAD` from disk on every keystroke.
pub fn current_branch(cwd: &Path) -> Option<String> {
    BRANCH_CACHE.with(|cache| {
        if let Some((read_at, branch)) = cache.borrow().get(cwd)
            && read_at.elapsed() < BRANCH_CACHE_TTL
        {
            return branch.clone();
        }
        let branch = read_branch(cwd);
        cache
            .borrow_mut()
            .insert(cwd.to_path_buf(), (Instant::now(), branch.clone()));
        branch
    })
}

/// Uncached disk read: find and parse `.git/HEAD` for `cwd`.
fn read_branch(cwd: &Path) -> Option<String> {
    let head_path = find_head(cwd)?;
    let raw = std::fs::read_to_string(&head_path).ok()?;
    parse_head(&raw)
}

/// Parse the contents of a `.git/HEAD` file. Split out so the
/// regression tests can exercise the parser without filesystem fixtures.
pub(crate) fn parse_head(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("ref: refs/heads/") {
        return Some(rest.to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("ref: ") {
        // Non-branch ref (e.g. refs/tags/<x>, refs/remotes/<x>). Use
        // the last path component as the label rather than the full ref.
        return rest.rsplit('/').next().map(String::from);
    }
    // Detached HEAD — raw SHA. Show short form.
    if trimmed.len() >= 7 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(trimmed[..7].to_string());
    }
    None
}

/// Walk up from `cwd` looking for a `.git` directory containing `HEAD`,
/// or a `.git` file (worktree / submodule) whose `gitdir:` line points
/// at the real gitdir. Returns the path to the resolved HEAD file.
fn find_head(cwd: &Path) -> Option<PathBuf> {
    let mut dir = cwd;
    loop {
        let git_path = dir.join(".git");
        if git_path.is_dir() {
            let head = git_path.join("HEAD");
            if head.is_file() {
                return Some(head);
            }
        } else if git_path.is_file()
            && let Ok(contents) = std::fs::read_to_string(&git_path)
            && let Some(rest) = contents.trim().strip_prefix("gitdir: ")
        {
            // Worktree / submodule: `.git` is a file containing
            // `gitdir: <path>`. Resolve and look for HEAD there.
            let gitdir = Path::new(rest);
            let resolved = if gitdir.is_absolute() {
                gitdir.to_path_buf()
            } else {
                dir.join(gitdir)
            };
            let head = resolved.join("HEAD");
            if head.is_file() {
                return Some(head);
            }
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => return None,
        }
    }
}
