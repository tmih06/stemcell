//! Tests for `rsi_git_history` — the suppress-stale-alerts helper that
//! lets the RSI cycle check whether a tool's failures already have a
//! fix commit since the window opened.
//!
//! Tests are hermetic: each one initializes a fresh git repo in a tmp
//! dir, makes the required commits with controlled timestamps, and
//! invokes the helper directly. No reliance on the host git config or
//! the opencrabs source repo state.

use crate::brain::rsi_git_history::{
    CommitSummary, commits_matching_since, parse_git_log_output, resolve_source_repo,
};
use std::path::Path;
use std::process::Command;

// ─── parse_git_log_output: pure parser, no git needed ───────────────

#[test]
fn parse_returns_empty_for_empty_input() {
    assert!(parse_git_log_output("").is_empty());
}

#[test]
fn parse_extracts_sha_and_subject_separated_by_tab() {
    let stdout = "abc1234567\tfix(provider): unwrap proxy error envelopes\n\
                  def0987654\tfix(browser): name the actual browser\n";
    let parsed = parse_git_log_output(stdout);
    assert_eq!(parsed.len(), 2);
    assert_eq!(
        parsed[0],
        CommitSummary {
            sha: "abc1234567".to_string(),
            subject: "fix(provider): unwrap proxy error envelopes".to_string(),
        }
    );
    assert_eq!(parsed[1].subject, "fix(browser): name the actual browser");
}

#[test]
fn parse_skips_lines_without_a_tab() {
    // Defensive: if git output ever contains a stray non-tab line
    // (trailing newline, error preamble that escaped stderr, etc.),
    // we drop those instead of producing CommitSummary with empty
    // fields and corrupting downstream comparisons.
    let stdout = "abc1234\tfix: real commit\n\
                  garbage line no tab\n\
                  def5678\tanother real one\n";
    let parsed = parse_git_log_output(stdout);
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].sha, "abc1234");
    assert_eq!(parsed[1].sha, "def5678");
}

#[test]
fn parse_skips_lines_with_empty_sha_or_subject() {
    let stdout = "\trogue subject without sha\n\
                  abc1234\t\n\
                  abc5678\treal\n";
    let parsed = parse_git_log_output(stdout);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].sha, "abc5678");
}

// ─── commits_matching_since: live git on a fixture repo ─────────────

/// Run `git` in `repo` with the given args, panicking with the captured
/// stderr if the command fails. Used to set up fixture repos.
fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("git available");
    if !out.status.success() {
        panic!(
            "git {:?} failed in {}: {}",
            args,
            repo.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
}

/// Make a commit on a fresh empty file with the given subject. Each
/// call also bumps GIT_AUTHOR_DATE / GIT_COMMITTER_DATE so we control
/// the commit timestamp independently of wall-clock.
fn commit_at(repo: &Path, subject: &str, iso_date: &str) {
    let stamp = format!("touch-{}", subject.replace([' ', ':', '/'], "-"));
    std::fs::write(repo.join(&stamp), b"").expect("touch fixture file");
    git(repo, &["add", &stamp]);
    let env = [
        ("GIT_AUTHOR_DATE", iso_date),
        ("GIT_COMMITTER_DATE", iso_date),
    ];
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["commit", "-m", subject])
        .envs(env)
        .output()
        .expect("git commit");
    assert!(
        out.status.success(),
        "git commit '{}' failed: {}",
        subject,
        String::from_utf8_lossy(&out.stderr).trim()
    );
}

/// Initialize a fresh repo in a tmp dir and configure identity / branch
/// so commits succeed deterministically across machines.
fn fixture_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tmpdir");
    let repo = dir.path();
    git(repo, &["init", "-q", "--initial-branch=main"]);
    git(repo, &["config", "user.email", "rsi-test@example.com"]);
    git(repo, &["config", "user.name", "RSI Test"]);
    git(repo, &["config", "commit.gpgsign", "false"]);
    dir
}

#[test]
fn matches_subject_term_within_window() {
    let dir = fixture_repo();
    let repo = dir.path();
    commit_at(repo, "init: bootstrap", "2026-04-20T00:00:00Z");
    commit_at(
        repo,
        "fix(browser): unstick navigation timeout",
        "2026-04-23T10:00:00Z",
    );
    commit_at(repo, "chore: bump version", "2026-04-25T08:00:00Z");

    let hits = commits_matching_since(repo, "2026-04-22T00:00:00Z", "browser");
    assert_eq!(
        hits.len(),
        1,
        "exactly one commit since 04-22 mentions 'browser': {hits:?}"
    );
    assert!(hits[0].subject.contains("unstick navigation timeout"));
}

#[test]
fn since_filter_excludes_older_matching_commits() {
    let dir = fixture_repo();
    let repo = dir.path();
    commit_at(
        repo,
        "fix(browser): old fix from before window",
        "2026-04-10T00:00:00Z",
    );
    commit_at(repo, "feat: unrelated", "2026-04-23T00:00:00Z");

    let hits = commits_matching_since(repo, "2026-04-20T00:00:00Z", "browser");
    assert!(
        hits.is_empty(),
        "old fix from 04-10 must not show with --since=04-20: {hits:?}"
    );
}

#[test]
fn grep_term_matches_case_insensitively() {
    // Tool names are lowercase (`exa_search`); commit subjects often
    // capitalize ("fix(EXA): foo"). Insensitive match is required so
    // we don't miss real fixes because of casing.
    let dir = fixture_repo();
    let repo = dir.path();
    commit_at(
        repo,
        "fix(EXA): handle stateless MCP",
        "2026-04-23T10:00:00Z",
    );

    let hits = commits_matching_since(repo, "2026-04-20T00:00:00Z", "exa");
    assert_eq!(hits.len(), 1, "case-insensitive grep must hit: {hits:?}");
}

#[test]
fn empty_when_no_commit_matches_term() {
    let dir = fixture_repo();
    let repo = dir.path();
    commit_at(
        repo,
        "fix(provider): retry transient 400s",
        "2026-04-23T10:00:00Z",
    );

    let hits = commits_matching_since(repo, "2026-04-20T00:00:00Z", "wait_agent");
    assert!(
        hits.is_empty(),
        "no commit mentions wait_agent → empty: {hits:?}"
    );
}

#[test]
fn merge_commits_excluded() {
    // The helper passes --no-merges. Verify a merge commit whose
    // subject contains the term is still excluded — merges add noise
    // and rarely represent a real fix.
    let dir = fixture_repo();
    let repo = dir.path();
    commit_at(repo, "init", "2026-04-20T00:00:00Z");
    git(repo, &["checkout", "-b", "feature", "-q"]);
    commit_at(repo, "feature: tweak browser thing", "2026-04-22T00:00:00Z");
    git(repo, &["checkout", "main", "-q"]);
    commit_at(repo, "main: unrelated activity", "2026-04-22T01:00:00Z");
    // Force a merge commit (no-ff) so its subject is a merge subject,
    // not the feature subject.
    let env = [
        ("GIT_AUTHOR_DATE", "2026-04-23T00:00:00Z"),
        ("GIT_COMMITTER_DATE", "2026-04-23T00:00:00Z"),
    ];
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "merge",
            "--no-ff",
            "feature",
            "-m",
            "Merge feature: includes browser fix",
        ])
        .envs(env)
        .output()
        .expect("git merge");
    assert!(
        out.status.success(),
        "merge failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );

    let hits = commits_matching_since(repo, "2026-04-20T00:00:00Z", "browser");
    // The feature commit DOES match (non-merge); the merge commit does
    // NOT (excluded by --no-merges). So we expect exactly the feature
    // commit.
    assert_eq!(hits.len(), 1, "expected only the feature commit: {hits:?}");
    assert!(hits[0].subject.contains("feature: tweak browser thing"));
}

#[test]
fn returns_empty_for_nonexistent_repo() {
    // Helper must not panic when the resolved path isn't a repo.
    let nope = Path::new("/tmp/opencrabs-rsi-nonexistent-xyz");
    let hits = commits_matching_since(nope, "2026-04-20T00:00:00Z", "browser");
    assert!(hits.is_empty());
}

// ─── resolve_source_repo: env override + cwd Cargo.toml fallback ────

#[test]
fn resolve_source_repo_returns_some_when_env_var_points_at_a_git_dir() {
    let dir = fixture_repo();
    // SAFETY: tests in this binary may run in parallel; the env var is
    // process-global. Snapshot and restore the previous value to avoid
    // bleeding state. This is the standard `temp-env`-free pattern.
    let prev = std::env::var("OPENCRABS_SRC").ok();
    // SAFETY: same parallelism caveat as above. The crate doesn't use
    // OPENCRABS_SRC anywhere else under cfg(test), so swapping it for
    // the duration of this test is safe in practice.
    unsafe { std::env::set_var("OPENCRABS_SRC", dir.path()) };

    let resolved = resolve_source_repo();
    assert_eq!(resolved.as_deref(), Some(dir.path()));

    match prev {
        Some(v) => unsafe { std::env::set_var("OPENCRABS_SRC", v) },
        None => unsafe { std::env::remove_var("OPENCRABS_SRC") },
    }
}

#[test]
fn resolve_source_repo_rejects_env_var_without_dot_git() {
    let dir = tempfile::tempdir().expect("tmpdir");
    // No `git init` here — the dir exists but isn't a repo.
    let prev = std::env::var("OPENCRABS_SRC").ok();
    unsafe { std::env::set_var("OPENCRABS_SRC", dir.path()) };

    // Resolution from env requires `.git/`; without it the helper must
    // fall through to the cwd-Cargo.toml path. We don't assert that
    // path here (it's machine-dependent), only that the env-only
    // result isn't honored.
    let resolved = resolve_source_repo();
    assert_ne!(
        resolved.as_deref(),
        Some(dir.path()),
        "non-repo env path must be rejected"
    );

    match prev {
        Some(v) => unsafe { std::env::set_var("OPENCRABS_SRC", v) },
        None => unsafe { std::env::remove_var("OPENCRABS_SRC") },
    }
}
