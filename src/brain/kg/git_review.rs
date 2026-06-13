//! Git backing for the knowledge-graph vault: versioning + a branch/review gate.
//!
//! The vault becomes a git repository so any approved state is restorable. Agent
//! memory writes are sealed onto a **batch branch** in a sibling worktree (outside
//! the watched vault root, so they trip no `notify` events), parked in a durable
//! queue, and merged into `main` only on explicit user approval.
//!
//! ## Why shell out
//!
//! The repo has no `git2`/`gix` dependency; the one existing git integration
//! ([`crate::brain::rsi_git_history`]) shells out to the `git` binary. We follow
//! that style: [`GitRepo`] runs `git -C <dir> …` via [`std::process::Command`],
//! parses stdout / exit codes, and degrades gracefully. Pure output parsers are
//! free functions so they unit-test without spawning git.
//!
//! ## Identity
//!
//! Every commit/merge passes `-c user.email=… -c user.name=… -c
//! commit.gpgsign=false` so the operation never depends on (or mutates) the
//! user's global git config, and never blocks on a GPG prompt.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Inline git identity flags applied to every commit/merge/revert so the vault's
/// history is self-contained and independent of global git config.
const IDENTITY: &[&str] = &[
    "-c",
    "user.email=stemcell@localhost",
    "-c",
    "user.name=StemCell",
    "-c",
    "commit.gpgsign=false",
];

/// `.gitignore` contents for a vault repo — keep Obsidian/editor noise out of
/// version control so batch diffs are pure note content.
const GITIGNORE: &str = ".obsidian/\n.trash/\n";

/// `.gitattributes` contents — the union merge driver on `*.md` is what makes
/// "auto append-merge" work: git keeps both sides' lines for non-conflicting
/// hunks, so two batches appending different bullets to the same note both land.
const GITATTRIBUTES: &str = "*.md merge=union\n";

/// A git diff summary (`git diff --shortstat` parsed).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffStat {
    pub files_changed: i64,
    pub insertions: i64,
    pub deletions: i64,
}

/// One commit as returned by [`GitRepo::log`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub sha: String,
    /// Committer date, ISO-8601 (`%cI`).
    pub date: String,
    pub subject: String,
}

/// Outcome of an attempted merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Merge committed cleanly. Carries the merge commit sha.
    Merged(String),
    /// Merge hit a true same-line conflict and was aborted. Carries the
    /// conflicted vault-relative paths.
    Conflicted(Vec<String>),
}

/// A handle to a git repository rooted at an absolute directory.
#[derive(Debug, Clone)]
pub struct GitRepo {
    root: PathBuf,
}

impl GitRepo {
    /// Open a repo handle at `root` (does not verify a repo exists — call
    /// [`is_repo`](Self::is_repo) / [`ensure_repo`](Self::ensure_repo)).
    pub fn open(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Run `git -C <root> <args…>`, returning `(stdout, stderr, success)`.
    /// `extra` flags (e.g. identity) are inserted before `args`.
    fn run(&self, args: &[&str]) -> Result<(String, String, bool)> {
        self.run_in(&self.root, args)
    }

    /// Like [`run`](Self::run) but with an explicit working directory (used for
    /// staging-worktree operations).
    fn run_in(&self, dir: &Path, args: &[&str]) -> Result<(String, String, bool)> {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .with_context(|| format!("failed to spawn git {args:?} in {dir:?}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        Ok((stdout, stderr, output.status.success()))
    }

    /// Run a git command and return stdout, erroring on a non-zero exit.
    fn run_ok(&self, args: &[&str]) -> Result<String> {
        let (stdout, stderr, ok) = self.run(args)?;
        if !ok {
            bail!("git {args:?} failed: {}", stderr.trim());
        }
        Ok(stdout)
    }

    /// True if `root` is itself the top level of a git working tree.
    ///
    /// We compare `--show-toplevel` to `root` rather than just checking
    /// `rev-parse` success, because the latter is true when *any* ancestor is a
    /// repo (e.g. `~` under version control) — which would make `ensure_repo`
    /// skip init and then operate on the wrong repository.
    pub fn is_repo(&self) -> bool {
        let Ok((stdout, _, true)) = self.run(&["rev-parse", "--show-toplevel"]) else {
            return false;
        };
        let toplevel = PathBuf::from(stdout.trim());
        match (toplevel.canonicalize(), self.root.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }

    /// Initialize the vault as a git repo if it isn't one already, writing
    /// `.gitignore` + `.gitattributes` and taking an initial snapshot commit.
    /// Idempotent: a no-op (returns `false`) when the repo already exists.
    pub fn ensure_repo(&self) -> Result<bool> {
        if self.is_repo() {
            return Ok(false);
        }
        self.run_ok(&["init"])?;
        std::fs::write(self.root.join(".gitignore"), GITIGNORE)
            .context("failed to write vault .gitignore")?;
        std::fs::write(self.root.join(".gitattributes"), GITATTRIBUTES)
            .context("failed to write vault .gitattributes")?;
        self.run_ok(&["add", "-A"])?;
        self.commit("chore(kg): initial vault snapshot")?;
        Ok(true)
    }

    /// Current HEAD sha of the repo.
    pub fn head_sha(&self) -> Result<String> {
        Ok(self.run_ok(&["rev-parse", "HEAD"])?.trim().to_string())
    }

    /// Commit all staged changes with `message` (identity-pinned). Assumes the
    /// caller already staged (`add -A`).
    pub fn commit(&self, message: &str) -> Result<String> {
        let mut args: Vec<&str> = IDENTITY.to_vec();
        args.extend_from_slice(&["commit", "-m", message, "--allow-empty"]);
        let (_, stderr, ok) = self.run(&args)?;
        if !ok {
            bail!("git commit failed: {}", stderr.trim());
        }
        self.head_sha()
    }

    /// Stage everything in the repo root and commit it (identity-pinned). Used
    /// for direct (non-batch) writes — the auto-commit path when git is enabled
    /// but the review gate is off.
    pub fn add_all_commit(&self, message: &str) -> Result<String> {
        self.run_ok(&["add", "-A"])?;
        self.commit(message)
    }

    /// Create batch branch `branch` checked out into a new worktree at
    /// `worktree`, based on current HEAD.
    pub fn create_batch_worktree(&self, branch: &str, worktree: &Path) -> Result<()> {
        let wt = worktree.to_string_lossy();
        self.run_ok(&["worktree", "add", "-b", branch, &wt, "HEAD"])?;
        Ok(())
    }

    /// Stage everything in a worktree dir and commit it (identity-pinned).
    pub fn commit_worktree(&self, worktree: &Path, message: &str) -> Result<String> {
        let (_, stderr, ok) = self.run_in(worktree, &["add", "-A"])?;
        if !ok {
            bail!("git add in worktree failed: {}", stderr.trim());
        }
        let mut args: Vec<&str> = IDENTITY.to_vec();
        args.extend_from_slice(&["commit", "-m", message, "--allow-empty"]);
        let (_, stderr, ok) = self.run_in(worktree, &args)?;
        if !ok {
            bail!("git commit in worktree failed: {}", stderr.trim());
        }
        let (stdout, stderr, ok) = self.run_in(worktree, &["rev-parse", "HEAD"])?;
        if !ok {
            bail!("git rev-parse in worktree failed: {}", stderr.trim());
        }
        Ok(stdout.trim().to_string())
    }

    /// Diff stats for a batch branch against its merge base with `base`.
    pub fn diff_stat(&self, base: &str, branch: &str) -> Result<DiffStat> {
        let range = format!("{base}...{branch}");
        let stdout = self.run_ok(&["diff", "--shortstat", &range])?;
        Ok(parse_shortstat(&stdout))
    }

    /// Full patch text for a batch branch against its merge base with `base`.
    pub fn diff_patch(&self, base: &str, branch: &str) -> Result<String> {
        let range = format!("{base}...{branch}");
        self.run_ok(&["diff", &range])
    }

    /// Merge a batch branch into the current branch with `--no-ff`. On a true
    /// same-line conflict the union driver can't resolve, abort and report the
    /// conflicted paths rather than leaving the tree half-merged.
    pub fn merge_batch(&self, branch: &str, message: &str) -> Result<MergeOutcome> {
        let mut args: Vec<&str> = IDENTITY.to_vec();
        args.extend_from_slice(&["merge", "--no-ff", branch, "-m", message]);
        let (_, _, ok) = self.run(&args)?;
        if ok {
            return Ok(MergeOutcome::Merged(self.head_sha()?));
        }
        // Capture conflicts, then abort so the working tree returns to a clean
        // state — the batch stays queued as `conflicted` for the user to resolve.
        let conflicts = self
            .run(&["diff", "--name-only", "--diff-filter=U"])
            .map(|(out, _, _)| parse_name_only(&out))
            .unwrap_or_default();
        let _ = self.run(&["merge", "--abort"]);
        Ok(MergeOutcome::Conflicted(conflicts))
    }

    /// Delete a batch branch (force) — used on decline.
    pub fn delete_branch(&self, branch: &str) -> Result<()> {
        self.run_ok(&["branch", "-D", branch])?;
        Ok(())
    }

    /// Remove a worktree (force) — used after approve/decline. Best-effort:
    /// a missing worktree is not an error.
    pub fn remove_worktree(&self, worktree: &Path) -> Result<()> {
        let wt = worktree.to_string_lossy();
        let _ = self.run(&["worktree", "remove", "--force", &wt]);
        Ok(())
    }

    /// Revert a merge commit (first-parent mainline), no-edit. Used by `/kg revert`.
    pub fn revert_merge(&self, merge_sha: &str) -> Result<String> {
        let mut args: Vec<&str> = IDENTITY.to_vec();
        args.extend_from_slice(&["revert", "-m", "1", "--no-edit", merge_sha]);
        let (_, stderr, ok) = self.run(&args)?;
        if !ok {
            bail!("git revert failed: {}", stderr.trim());
        }
        self.head_sha()
    }

    /// Hard-reset the vault working tree + HEAD to `sha`. Destructive — the TUI
    /// confirms before calling, and the caller runs it under watcher suppression
    /// followed by a full reindex.
    pub fn reset_hard(&self, sha: &str) -> Result<()> {
        self.run_ok(&["reset", "--hard", sha])?;
        Ok(())
    }

    /// Recent commits, newest first.
    pub fn log(&self, limit: usize) -> Result<Vec<LogEntry>> {
        let n = format!("-n{}", limit.max(1));
        let stdout = self.run_ok(&["log", "--format=%H%x09%cI%x09%s", &n])?;
        Ok(parse_log(&stdout))
    }

    /// Full `git show` for a commit (stat + patch).
    pub fn show(&self, sha: &str) -> Result<String> {
        self.run_ok(&["show", sha])
    }
}

/// Parse `git diff --shortstat` output, e.g.
/// `" 3 files changed, 12 insertions(+), 4 deletions(-)"`. All fields optional.
pub(crate) fn parse_shortstat(s: &str) -> DiffStat {
    let mut stat = DiffStat::default();
    for part in s.trim().split(',') {
        let p = part.trim();
        let Some((num, rest)) = p.split_once(' ') else {
            continue;
        };
        let Ok(n) = num.trim().parse::<i64>() else {
            continue;
        };
        if rest.contains("file") {
            stat.files_changed = n;
        } else if rest.contains("insertion") {
            stat.insertions = n;
        } else if rest.contains("deletion") {
            stat.deletions = n;
        }
    }
    stat
}

/// Parse `git diff --name-only` output into a list of paths.
pub(crate) fn parse_name_only(s: &str) -> Vec<String> {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// Parse `git log --format=%H%x09%cI%x09%s` (tab-separated sha/date/subject).
pub(crate) fn parse_log(s: &str) -> Vec<LogEntry> {
    s.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let sha = parts.next()?.trim();
            let date = parts.next()?.trim();
            let subject = parts.next().unwrap_or("").trim();
            if sha.is_empty() {
                return None;
            }
            Some(LogEntry {
                sha: sha.to_string(),
                date: date.to_string(),
                subject: subject.to_string(),
            })
        })
        .collect()
}
