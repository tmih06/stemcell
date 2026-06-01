//! Regression tests for the brain-file dedup scan and proposal flow.
//!
//! Covers: duplicate detection across files, near-duplicate detection,
//! periodicity gating, proposal format correctness, empty brain files
//! handled, files under min size skipped.

use crate::brain::dedup_scan::{
    DuplicateCluster, cluster_to_proposal, file_dedup_proposals, generate_dedup_proposals,
    scan_brain_files,
};
use crate::brain::rsi_proposals::ProposalsStore;
use std::fs;
use tempfile::TempDir;

fn write_brain_file(dir: &std::path::Path, name: &str, content: &str) {
    fs::write(dir.join(name), content).unwrap();
}

// --- Periodicity gating ---

#[test]
fn test_scan_does_not_fire_on_empty_brain_dir() {
    let dir = TempDir::new().unwrap();
    // No brain files at all
    let proposals = generate_dedup_proposals(dir.path());
    assert!(proposals.is_empty());
}

#[test]
fn test_scan_handles_missing_files_gracefully() {
    let dir = TempDir::new().unwrap();
    // Write only SOUL.md, all other brain files are missing
    write_brain_file(
        dir.path(),
        "SOUL.md",
        "# SOUL.md\n\nThis is unique content that appears once.\n",
    );
    let proposals = generate_dedup_proposals(dir.path());
    assert!(proposals.is_empty());
}

// --- Empty brain files handled ---

#[test]
fn test_empty_brain_file_produces_no_proposals() {
    let dir = TempDir::new().unwrap();
    write_brain_file(dir.path(), "SOUL.md", "");
    write_brain_file(dir.path(), "AGENTS.md", "");
    let proposals = generate_dedup_proposals(dir.path());
    assert!(proposals.is_empty());
}

#[test]
fn test_whitespace_only_brain_file_produces_no_proposals() {
    let dir = TempDir::new().unwrap();
    write_brain_file(dir.path(), "SOUL.md", "\n\n\n   \n\n");
    let proposals = generate_dedup_proposals(dir.path());
    assert!(proposals.is_empty());
}

// --- Files under min size skipped ---

#[test]
fn test_short_lines_are_skipped() {
    let dir = TempDir::new().unwrap();
    // Lines shorter than MIN_LINE_LEN (10 chars) should be ignored
    let content = "# SOUL.md\n\nshort\n\nshort\n\nshort\n";
    write_brain_file(dir.path(), "SOUL.md", content);
    let clusters = scan_brain_files(dir.path());
    assert!(clusters.is_empty(), "Short lines should be skipped");
}

#[test]
fn test_only_heading_lines_produces_no_clusters() {
    let dir = TempDir::new().unwrap();
    let content = "# Heading One\n\n# Heading Two\n\n# Heading Three\n";
    write_brain_file(dir.path(), "SOUL.md", content);
    let clusters = scan_brain_files(dir.path());
    assert!(clusters.is_empty());
}

// --- Duplicate detection across files ---

#[test]
fn test_duplicate_across_three_files() {
    let dir = TempDir::new().unwrap();
    let shared = "This rule about git commits is shared across multiple brain files for emphasis.";
    write_brain_file(
        dir.path(),
        "SOUL.md",
        &format!("# SOUL\n\n{}\n\nUnique soul stuff.\n", shared),
    );
    write_brain_file(
        dir.path(),
        "AGENTS.md",
        &format!("# AGENTS\n\n{}\n\nUnique agents stuff.\n", shared),
    );
    write_brain_file(
        dir.path(),
        "MEMORY.md",
        &format!("# MEMORY\n\n{}\n\nUnique memory stuff.\n", shared),
    );

    let clusters = scan_brain_files(dir.path());
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].total_count, 3);
    assert_eq!(clusters[0].locations.len(), 3);
}

// --- Proposal format correctness ---

#[test]
fn test_proposal_has_correct_fields() {
    let dir = TempDir::new().unwrap();
    let repeated = "Always run cargo clippy and cargo fmt before every commit without exception.";
    let content = format!(
        "# SOUL.md\n\n{}\n\nUnique line one.\n\n{}\n\nUnique line two.\n",
        repeated, repeated
    );
    write_brain_file(dir.path(), "SOUL.md", &content);

    let proposals = generate_dedup_proposals(dir.path());
    assert_eq!(proposals.len(), 1);

    let (proposal, rationale) = &proposals[0];
    assert_eq!(proposal.target_file, "SOUL.md");
    assert!(!proposal.duplicate_text.is_empty());
    assert!(!proposal.line_range.is_empty());
    assert!(proposal.duplicate_of.starts_with("SOUL.md:"));
    assert!(proposal.count >= 1);
    assert!(!rationale.is_empty());
}

#[test]
fn test_proposal_count_reflects_removable_duplicates() {
    let dir = TempDir::new().unwrap();
    let repeated = "Never use em dashes in any output or code comments at all.";
    // 4 occurrences — keep 1 canonical, remove 3
    let content = format!(
        "# SOUL.md\n\n{}\n\nA\n\n{}\n\nB\n\n{}\n\nC\n\n{}\n",
        repeated, repeated, repeated, repeated
    );
    write_brain_file(dir.path(), "SOUL.md", &content);

    let proposals = generate_dedup_proposals(dir.path());
    assert_eq!(proposals.len(), 1);
    let (proposal, _) = &proposals[0];
    assert_eq!(
        proposal.count, 3,
        "Should count 3 removable duplicates (4 total - 1 canonical)"
    );
}

// --- ProposalsStore integration ---

#[test]
fn test_filed_proposals_appear_in_store() {
    let brain_dir = TempDir::new().unwrap();
    let rsi_dir = TempDir::new().unwrap();

    let repeated = "Keep responses under 3 sentences when possible in all contexts.";
    let content = format!(
        "# SOUL.md\n\n{}\n\nUnique.\n\n{}\n\nMore unique.\n",
        repeated, repeated
    );
    write_brain_file(brain_dir.path(), "SOUL.md", &content);

    let store = ProposalsStore::with_dir(rsi_dir.path().to_path_buf());
    let count = file_dedup_proposals(brain_dir.path(), &store);

    assert!(count > 0);
    let pending = store.list_brain_dedup_proposals();
    assert_eq!(pending.len(), count);

    // Verify proposal format in store
    let p = &pending[0];
    assert!(p.id.starts_with("prop_dedup_"));
    assert_eq!(p.proposer, "rsi-dedup-scan");
    assert!(!p.rationale.is_empty());
    assert_eq!(p.dedup.target_file, "SOUL.md");
}

#[test]
fn test_repeated_scan_dedups_proposals_in_store() {
    let brain_dir = TempDir::new().unwrap();
    let rsi_dir = TempDir::new().unwrap();

    let repeated = "Never push tags or releases without explicit user approval at all.";
    let content = format!(
        "# SOUL.md\n\n{}\n\nUnique.\n\n{}\n\nMore unique.\n",
        repeated, repeated
    );
    write_brain_file(brain_dir.path(), "SOUL.md", &content);

    let store = ProposalsStore::with_dir(rsi_dir.path().to_path_buf());

    // First scan
    let count1 = file_dedup_proposals(brain_dir.path(), &store);
    assert!(count1 > 0);

    // Second scan — should NOT create duplicate proposals (dedup by target_file + duplicate_text)
    let count2 = file_dedup_proposals(brain_dir.path(), &store);
    assert_eq!(
        count2, count1,
        "Second scan should supersede, not duplicate"
    );

    let pending = store.list_brain_dedup_proposals();
    assert_eq!(
        pending.len(),
        count1,
        "Store should have exactly count1 proposals"
    );
}

// --- cluster_to_proposal edge cases ---

#[test]
fn test_cluster_with_single_occurrence_returns_none() {
    let cluster = DuplicateCluster {
        text: "Unique line that only appears once in the whole codebase.".to_string(),
        locations: vec![("SOUL.md".to_string(), vec![42])],
        total_count: 1,
    };
    let proposal = cluster_to_proposal(&cluster);
    assert!(
        proposal.is_none(),
        "Single occurrence should not generate a proposal"
    );
}

#[test]
fn test_cluster_with_empty_locations_returns_none() {
    let cluster = DuplicateCluster {
        text: "Some text here that is long enough to matter for scanning.".to_string(),
        locations: vec![],
        total_count: 0,
    };
    let proposal = cluster_to_proposal(&cluster);
    assert!(proposal.is_none());
}

// --- Cross-file canonical selection ---

#[test]
fn test_canonical_file_is_first_alphabetically() {
    let dir = TempDir::new().unwrap();
    let shared = "This rule is shared between AGENTS and SOUL files for testing purposes.";
    write_brain_file(
        dir.path(),
        "SOUL.md",
        &format!("# SOUL\n\n{}\n\nSoul unique.\n", shared),
    );
    write_brain_file(
        dir.path(),
        "AGENTS.md",
        &format!("# AGENTS\n\n{}\n\nAgents unique.\n", shared),
    );

    let proposals = generate_dedup_proposals(dir.path());
    assert_eq!(proposals.len(), 1);
    let (proposal, _) = &proposals[0];
    // AGENTS.md comes before SOUL.md alphabetically, so AGENTS.md is canonical
    // and SOUL.md is the target for removal
    assert_eq!(proposal.target_file, "SOUL.md");
    assert!(proposal.duplicate_of.starts_with("AGENTS.md:"));
}
