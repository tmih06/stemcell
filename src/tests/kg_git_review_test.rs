//! Tests for the knowledge-graph git backing (`brain::kg::git_review`).
//!
//! The flow tests spawn the real `git` binary against a `tempfile` repo; they
//! skip (not fail) when git is unavailable so the suite still runs in a
//! git-less sandbox. The pure-parser tests never spawn anything.

use crate::brain::kg::git_review::{
    DiffStat, GitRepo, LogEntry, MergeOutcome, parse_log, parse_name_only, parse_shortstat,
};

/// True if a `git` binary is on PATH — flow tests early-return otherwise.
fn git_available() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn ensure_repo_is_idempotent_and_scaffolds() {
    if !git_available() {
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = GitRepo::open(dir.path());
    assert!(!repo.is_repo(), "fresh dir is not a repo");

    assert!(repo.ensure_repo().expect("init"), "first call initializes");
    assert!(repo.is_repo(), "now a repo");
    assert!(dir.path().join(".gitignore").exists(), ".gitignore written");
    assert!(
        dir.path().join(".gitattributes").exists(),
        ".gitattributes written"
    );
    assert!(
        !repo.ensure_repo().expect("second"),
        "second call is a no-op"
    );
}

#[test]
fn batch_branch_queues_diff_then_merges_clean() {
    if !git_available() {
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = GitRepo::open(dir.path());
    repo.ensure_repo().expect("init");
    let base = repo.head_sha().expect("head");

    // Stage a batch in a sibling worktree.
    let wt = dir.path().parent().unwrap().join("stage-1");
    repo.create_batch_worktree("kg/batch/1", &wt)
        .expect("worktree");
    std::fs::write(wt.join("a.md"), "# A\n\n## Observations\n- [fact] one\n").expect("write");
    repo.commit_worktree(&wt, "remember A").expect("commit");

    let stat = repo.diff_stat(&base, "kg/batch/1").expect("stat");
    assert_eq!(stat.files_changed, 1, "one file in the batch");
    assert!(stat.insertions > 0);

    match repo.merge_batch("kg/batch/1", "approve A").expect("merge") {
        MergeOutcome::Merged(sha) => assert!(!sha.is_empty(), "merge sha returned"),
        MergeOutcome::Conflicted(c) => panic!("unexpected conflict: {c:?}"),
    }
    assert!(dir.path().join("a.md").exists(), "approved note is on main");
    repo.remove_worktree(&wt).expect("cleanup");
}

#[test]
fn union_driver_keeps_both_appends() {
    if !git_available() {
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = GitRepo::open(dir.path());
    repo.ensure_repo().expect("init");

    // Seed a note on main with one observation.
    std::fs::write(
        dir.path().join("n.md"),
        "# N\n\n## Observations\n- [fact] base\n",
    )
    .expect("seed");
    repo.add_all_commit("seed n").expect("seed commit");
    let base = repo.head_sha().expect("head");

    // Two batches each append a *different* observation to the same section.
    let wt1 = dir.path().parent().unwrap().join("u1");
    repo.create_batch_worktree("kg/batch/u1", &wt1)
        .expect("wt1");
    std::fs::write(
        wt1.join("n.md"),
        "# N\n\n## Observations\n- [fact] base\n- [fact] from-one\n",
    )
    .expect("w1");
    repo.commit_worktree(&wt1, "add one").expect("c1");

    let wt2 = dir.path().parent().unwrap().join("u2");
    repo.create_batch_worktree("kg/batch/u2", &wt2)
        .expect("wt2");
    std::fs::write(
        wt2.join("n.md"),
        "# N\n\n## Observations\n- [fact] base\n- [fact] from-two\n",
    )
    .expect("w2");
    repo.commit_worktree(&wt2, "add two").expect("c2");

    // Merge both. The second merges against a main that already has the first;
    // the union driver must keep both new lines rather than conflict.
    assert!(matches!(
        repo.merge_batch("kg/batch/u1", "approve u1").expect("m1"),
        MergeOutcome::Merged(_)
    ));
    assert!(
        matches!(
            repo.merge_batch("kg/batch/u2", "approve u2").expect("m2"),
            MergeOutcome::Merged(_)
        ),
        "union driver auto-merges non-overlapping appends"
    );

    let merged = std::fs::read_to_string(dir.path().join("n.md")).expect("read");
    assert!(merged.contains("from-one"), "kept batch one's append");
    assert!(merged.contains("from-two"), "kept batch two's append");
    let _ = base;
    repo.remove_worktree(&wt1).ok();
    repo.remove_worktree(&wt2).ok();
}

#[test]
fn revert_undoes_an_approved_merge() {
    if !git_available() {
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = GitRepo::open(dir.path());
    repo.ensure_repo().expect("init");

    let wt = dir.path().parent().unwrap().join("rev-1");
    repo.create_batch_worktree("kg/batch/r1", &wt).expect("wt");
    std::fs::write(wt.join("r.md"), "# R\nbody\n").expect("w");
    repo.commit_worktree(&wt, "remember R").expect("c");

    let merge_sha = match repo.merge_batch("kg/batch/r1", "approve R").expect("merge") {
        MergeOutcome::Merged(sha) => sha,
        MergeOutcome::Conflicted(_) => panic!("unexpected conflict"),
    };
    assert!(dir.path().join("r.md").exists(), "note present after merge");

    repo.revert_merge(&merge_sha).expect("revert");
    assert!(
        !dir.path().join("r.md").exists(),
        "note gone after reverting the merge"
    );
    repo.remove_worktree(&wt).ok();
}

#[test]
fn parse_shortstat_extracts_counts() {
    assert_eq!(
        parse_shortstat(" 3 files changed, 12 insertions(+), 4 deletions(-)"),
        DiffStat {
            files_changed: 3,
            insertions: 12,
            deletions: 4
        }
    );
    // Insertions only.
    assert_eq!(
        parse_shortstat(" 1 file changed, 2 insertions(+)"),
        DiffStat {
            files_changed: 1,
            insertions: 2,
            deletions: 0
        }
    );
    assert_eq!(parse_shortstat(""), DiffStat::default());
}

#[test]
fn parse_log_splits_sha_date_subject() {
    let out = "abc123\t2026-06-13T10:00:00+00:00\tkg: approve rust\n\
               def456\t2026-06-12T09:00:00+00:00\tchore(kg): initial vault snapshot\n";
    assert_eq!(
        parse_log(out),
        vec![
            LogEntry {
                sha: "abc123".into(),
                date: "2026-06-13T10:00:00+00:00".into(),
                subject: "kg: approve rust".into()
            },
            LogEntry {
                sha: "def456".into(),
                date: "2026-06-12T09:00:00+00:00".into(),
                subject: "chore(kg): initial vault snapshot".into()
            },
        ]
    );
}

#[test]
fn parse_name_only_lists_paths() {
    assert_eq!(
        parse_name_only("concepts/A.md\npeople/B.md\n\n"),
        vec!["concepts/A.md".to_string(), "people/B.md".to_string()]
    );
    assert!(parse_name_only("").is_empty());
}
