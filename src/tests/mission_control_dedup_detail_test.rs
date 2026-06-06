//! Tests for the dedup detail enrichment in Mission Control inbox.
//!
//! Verifies that `inbox_service::list_with_store` populates the
//! `McInboxItem::detail` field for brain dedup proposals and leaves
//! it `None` for tool/command/skill proposals.

use crate::brain::mission_control::{McInboxDetail, McInboxKind, inbox_service};
use crate::brain::rsi_proposals::{ProposalsStore, ProposedBrainDedup};
use tempfile::TempDir;

fn sample_dedup(file: &str, text: &str) -> ProposedBrainDedup {
    ProposedBrainDedup {
        target_file: file.to_string(),
        duplicate_text: text.to_string(),
        line_range: "23-25".to_string(),
        duplicate_of: format!("{file}:14"),
        count: 1,
        warnings: Vec::new(),
    }
}

fn sample_dedup_with_warnings(file: &str, text: &str, warnings: Vec<&str>) -> ProposedBrainDedup {
    ProposedBrainDedup {
        target_file: file.to_string(),
        duplicate_text: text.to_string(),
        line_range: "100-105".to_string(),
        duplicate_of: format!("{file}:42"),
        count: 2,
        warnings: warnings.into_iter().map(String::from).collect(),
    }
}

#[test]
fn brain_dedup_item_has_detail_populated() {
    let dir = TempDir::new().unwrap();
    let store = ProposalsStore::with_dir(dir.path().to_path_buf());
    store
        .add_brain_dedup_proposal(
            "rsi-dedup-scan",
            "Exact duplicate of Conciseness block",
            sample_dedup(
                "SOUL.md",
                "## Conciseness\nKeep responses under 3 sentences.",
            ),
        )
        .unwrap();

    let items = inbox_service::list_with_store(&store);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].kind, McInboxKind::ProposedBrainDedup);

    let detail = items[0]
        .detail
        .as_ref()
        .expect("dedup item must have detail");
    match detail {
        McInboxDetail::BrainDedup {
            duplicate_text,
            rationale,
            duplicate_of,
            warnings,
        } => {
            assert_eq!(
                duplicate_text,
                "## Conciseness\nKeep responses under 3 sentences."
            );
            assert_eq!(rationale, "Exact duplicate of Conciseness block");
            assert_eq!(duplicate_of, "SOUL.md:14");
            assert!(warnings.is_empty());
        }
    }
}

#[test]
fn brain_dedup_detail_carries_warnings() {
    let dir = TempDir::new().unwrap();
    let store = ProposalsStore::with_dir(dir.path().to_path_buf());
    store
        .add_brain_dedup_proposal(
            "rsi-dedup-scan",
            "Repeated section body",
            sample_dedup_with_warnings(
                "AGENTS.md",
                "Some duplicated lines here",
                vec!["## Git Rules would become a stub"],
            ),
        )
        .unwrap();

    let items = inbox_service::list_with_store(&store);
    let detail = items[0]
        .detail
        .as_ref()
        .expect("dedup item must have detail");
    match detail {
        McInboxDetail::BrainDedup { warnings, .. } => {
            assert_eq!(warnings.len(), 1);
            assert_eq!(warnings[0], "## Git Rules would become a stub");
        }
    }
}

#[test]
fn tool_proposal_has_no_detail() {
    use crate::brain::tools::dynamic::tool::{DynamicToolDef, ExecutorType};
    let dir = TempDir::new().unwrap();
    let store = ProposalsStore::with_dir(dir.path().to_path_buf());
    store
        .add_tool_proposal(
            "rsi-autonomous",
            "frequent use",
            DynamicToolDef {
                name: "test_tool".to_string(),
                description: "test".to_string(),
                executor: ExecutorType::Shell,
                enabled: true,
                requires_approval: true,
                method: None,
                url: None,
                headers: Default::default(),
                timeout_secs: 30,
                command: Some("echo hi".to_string()),
                params: Vec::new(),
            },
        )
        .unwrap();

    let items = inbox_service::list_with_store(&store);
    assert_eq!(items.len(), 1);
    assert!(
        items[0].detail.is_none(),
        "tool proposals should have no detail"
    );
}

#[test]
fn command_proposal_has_no_detail() {
    use crate::brain::commands::UserCommand;
    let dir = TempDir::new().unwrap();
    let store = ProposalsStore::with_dir(dir.path().to_path_buf());
    store
        .add_command_proposal(
            "rsi-autonomous",
            "user typed /deploy",
            UserCommand {
                name: "/deploy".to_string(),
                description: String::new(),
                action: "prompt".to_string(),
                prompt: "Deploy to staging".to_string(),
            },
        )
        .unwrap();

    let items = inbox_service::list_with_store(&store);
    assert_eq!(items.len(), 1);
    assert!(
        items[0].detail.is_none(),
        "command proposals should have no detail"
    );
}

#[test]
fn dedup_summary_still_shows_count_and_range() {
    let dir = TempDir::new().unwrap();
    let store = ProposalsStore::with_dir(dir.path().to_path_buf());
    store
        .add_brain_dedup_proposal(
            "rsi-dedup-scan",
            "duplicate block",
            sample_dedup("USER.md", "Some text"),
        )
        .unwrap();

    let items = inbox_service::list_with_store(&store);
    assert!(
        items[0].summary.contains("1 duplicate(s)"),
        "summary should still show count: got {:?}",
        items[0].summary
    );
    assert!(
        items[0].summary.contains("23-25"),
        "summary should still show line range: got {:?}",
        items[0].summary
    );
}
