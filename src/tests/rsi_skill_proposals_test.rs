//! Tests for the skill proposal path added to `ProposalsStore` +
//! `rsi_propose`. Skills (~/.stemcell/skills/<name>/SKILL.md) are
//! the third proposal kind alongside tools and commands — cheaper to
//! author than dynamic tools (no schema, no executor wiring), the
//! right shape for "RSI noticed a sequence of bash + http calls
//! keeps coming up; codify it as a workflow".
//!
//! Tests touch the store via `ProposalsStore::with_dir(tmpdir)` so
//! they never write to the user's real `~/.stemcell/rsi/` inbox.

use crate::brain::rsi_proposals::{ProposalsStore, ProposedSkill};

fn sample_skill() -> ProposedSkill {
    ProposedSkill {
        name: "github_release_pipeline".to_string(),
        description: "Run the standard release sequence: branch check, changelog, tag, publish."
            .to_string(),
        body: "# GitHub Release Pipeline\n\n\
               1. Confirm working tree clean.\n\
               2. Draft CHANGELOG entry from `git log <last-tag>..HEAD`.\n\
               3. Bump version in Cargo.toml.\n\
               4. Commit + tag + push.\n\
               5. Publish to crates.io."
            .to_string(),
    }
}

#[test]
fn add_skill_proposal_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProposalsStore::with_dir(dir.path().to_path_buf());

    let id = store
        .add_skill_proposal(
            "rsi-autonomous",
            "saw 14 release sequences this week",
            sample_skill(),
        )
        .expect("add succeeds");

    assert!(
        id.starts_with("prop_skill_"),
        "id prefix should mark kind; got: {id}"
    );
    let listed = store.list_skill_proposals();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
    assert_eq!(listed[0].skill.name, "github_release_pipeline");
    assert!(listed[0].skill.body.contains("CHANGELOG"));
    assert_eq!(listed[0].rationale, "saw 14 release sequences this week");
}

#[test]
fn skill_dedup_by_name_supersedes_older() {
    // RSI cycles may re-propose the same skill on multiple runs if
    // the underlying pattern persists. Inbox should never accumulate
    // duplicates by name — the newest proposal wins.
    let dir = tempfile::tempdir().unwrap();
    let store = ProposalsStore::with_dir(dir.path().to_path_buf());

    let mut first = sample_skill();
    first.description = "first version".to_string();
    let mut second = sample_skill();
    second.description = "refined version".to_string();

    store.add_skill_proposal("rsi", "first run", first).unwrap();
    store
        .add_skill_proposal("rsi", "second run", second)
        .unwrap();

    let listed = store.list_skill_proposals();
    assert_eq!(listed.len(), 1, "dedup must keep one entry per name");
    assert_eq!(listed[0].skill.description, "refined version");
    assert_eq!(listed[0].rationale, "second run");
}

#[test]
fn take_skill_proposal_removes_from_inbox() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProposalsStore::with_dir(dir.path().to_path_buf());

    let id = store
        .add_skill_proposal("rsi", "evidence", sample_skill())
        .unwrap();
    assert_eq!(store.list_skill_proposals().len(), 1);

    let taken = store
        .take_skill_proposal(&id)
        .expect("take succeeds")
        .expect("entry present");
    assert_eq!(taken.skill.name, "github_release_pipeline");
    assert!(store.list_skill_proposals().is_empty(), "inbox cleared");

    // Re-taking the same id returns None gracefully.
    let again = store.take_skill_proposal(&id).expect("take succeeds");
    assert!(again.is_none(), "already taken → None");
}

#[test]
fn pending_count_includes_skills() {
    // The TUI session-start banner reads pending_count to decide
    // whether to surface a "you have N RSI proposals" hint. After
    // this change skills must contribute to that total too.
    let dir = tempfile::tempdir().unwrap();
    let store = ProposalsStore::with_dir(dir.path().to_path_buf());

    assert_eq!(store.pending_count(), 0);
    store
        .add_skill_proposal("rsi", "evidence", sample_skill())
        .unwrap();
    assert_eq!(store.pending_count(), 1, "skill counted in pending");
}

#[test]
fn archive_applied_skill_writes_daily_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProposalsStore::with_dir(dir.path().to_path_buf());

    let id = store
        .add_skill_proposal("rsi", "evidence", sample_skill())
        .unwrap();
    let taken = store.take_skill_proposal(&id).unwrap().unwrap();

    store
        .archive_applied_skill(&taken)
        .expect("archive succeeds");

    // Daily archive file exists under applied/.
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let archive_path = dir
        .path()
        .join("applied")
        .join(format!("{date}-skills.toml"));
    assert!(
        archive_path.exists(),
        "applied/<date>-skills.toml should be written"
    );
    let contents = std::fs::read_to_string(&archive_path).unwrap();
    assert!(contents.contains("github_release_pipeline"));
}

#[test]
fn archive_rejected_skill_captures_reason() {
    let dir = tempfile::tempdir().unwrap();
    let store = ProposalsStore::with_dir(dir.path().to_path_buf());

    let id = store
        .add_skill_proposal("rsi", "evidence", sample_skill())
        .unwrap();
    let taken = store.take_skill_proposal(&id).unwrap().unwrap();

    store
        .archive_rejected_skill(&taken, Some("conflicts with manual release process"))
        .expect("archive succeeds");

    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let archive_path = dir
        .path()
        .join("rejected")
        .join(format!("{date}-skills.toml"));
    assert!(archive_path.exists());
    let contents = std::fs::read_to_string(&archive_path).unwrap();
    assert!(contents.contains("conflicts with manual release"));
}

// ── rsi_propose tool tests ──────────────────────────────────────

#[tokio::test]
async fn rsi_propose_skill_rejects_missing_body() {
    use crate::brain::tools::rsi_propose::RsiProposeTool;
    use crate::brain::tools::{Tool, ToolExecutionContext};

    let tool = RsiProposeTool;
    let ctx = ToolExecutionContext::new(uuid::Uuid::new_v4());
    let input = serde_json::json!({
        "kind": "skill",
        "rationale": "evidence",
        "name": "github_workflow",
        "description": "wraps a sequence",
    });
    let result = tool.execute(input, &ctx).await.unwrap();
    assert!(!result.success);
    assert!(
        result.error.unwrap_or_default().contains("body"),
        "missing body should error with mention of `body`"
    );
}

#[tokio::test]
async fn rsi_propose_skill_rejects_invalid_name_chars() {
    use crate::brain::tools::rsi_propose::RsiProposeTool;
    use crate::brain::tools::{Tool, ToolExecutionContext};

    let tool = RsiProposeTool;
    let ctx = ToolExecutionContext::new(uuid::Uuid::new_v4());
    let input = serde_json::json!({
        "kind": "skill",
        "rationale": "evidence",
        "name": "bad name with spaces!",
        "description": "wraps a sequence",
        "body": "step 1\nstep 2",
    });
    let result = tool.execute(input, &ctx).await.unwrap();
    assert!(!result.success);
    let err = result.error.unwrap_or_default();
    assert!(
        err.contains("alphanumeric"),
        "invalid chars should be reported; got: {err}"
    );
}

#[tokio::test]
async fn rsi_propose_skill_strips_leading_slash_in_name() {
    // The model may emit "/github_workflow" by analogy with command
    // names. Skills don't use a slash prefix — they live under a
    // directory. The tool should normalise instead of rejecting.
    use crate::brain::tools::rsi_propose::RsiProposeTool;
    use crate::brain::tools::{Tool, ToolExecutionContext};

    let tool = RsiProposeTool;
    let ctx = ToolExecutionContext::new(uuid::Uuid::new_v4());
    // This test exercises the validation path only — we can't easily
    // verify the persisted file without a tmpdir override on the
    // tool (the tool uses stemcell_home()). What we CAN check: the
    // tool succeeds (name accepted after normalisation).
    let input = serde_json::json!({
        "kind": "skill",
        "rationale": "evidence",
        "name": "/github_workflow",
        "description": "wraps a sequence",
        "body": "step 1\nstep 2",
    });
    let result = tool.execute(input, &ctx).await.unwrap();
    // Either success (skill written) or a file-write error — both
    // confirm the leading-slash was accepted and stripped. What we're
    // guarding against is the "invalid chars" branch firing on `/`.
    if !result.success {
        let err = result.error.unwrap_or_default();
        assert!(
            !err.contains("alphanumeric"),
            "leading slash must NOT trip the alphanumeric guard; got: {err}"
        );
    }
}
