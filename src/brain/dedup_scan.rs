//! Periodic brain-file deduplication scanner.
//!
//! Reads all brain files (SOUL.md, AGENTS.md, MEMORY.md, etc.) and
//! identifies exact duplicate lines or near-duplicate blocks. Results
//! are converted into `BrainDedupProposal` entries for Mission Control
//! review.
//!
//! The scanner is conservative: it only flags duplicates that are
//! clearly redundant (exact line matches, repeated blocks) and skips
//! structural markdown (headings, blank lines, separators). This
//! avoids false positives on intentional repetition (e.g., numbered
//! lists with similar prefixes).

use std::collections::HashMap;
use std::path::Path;

use crate::brain::prompt_builder::CONTEXTUAL_BRAIN_FILES;
use crate::brain::rsi_proposals::ProposedBrainDedup;

/// Core brain files to scan (both CORE and CONTEXTUAL).
const BRAIN_FILES_TO_SCAN: &[&str] = &[
    "SOUL.md",
    "USER.md",
    "IDENTITY.md",
    "AGENTS.md",
    "CODE.md",
    "TOOLS.md",
    "SECURITY.md",
    "MEMORY.md",
    "BOOT.md",
    "BOOTSTRAP.md",
    "HEARTBEAT.md",
];

/// Minimum line length to consider for dedup (skip short structural lines).
const MIN_LINE_LEN: usize = 10;

/// Minimum occurrences to flag as duplicate.
const MIN_DUPLICATE_COUNT: usize = 2;

/// Lines that look like markdown structure — skip these.
fn is_structural_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.len() < MIN_LINE_LEN {
        return true;
    }
    // Headings
    if trimmed.starts_with('#') {
        return true;
    }
    // Horizontal rules
    if trimmed.chars().all(|c| c == '-' || c == '=' || c == '*' || c == '_') {
        return true;
    }
    // Table separators
    if trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.contains("---") {
        return true;
    }
    // Blockquotes with short content
    if trimmed.starts_with('>') && trimmed.len() < 20 {
        return true;
    }
    false
}

/// One cluster of duplicate content found by the scan.
#[derive(Debug, Clone)]
pub struct DuplicateCluster {
    /// The duplicated text (one instance).
    pub text: String,
    /// Where it appears: (filename, line_numbers 1-indexed).
    pub locations: Vec<(String, Vec<usize>)>,
    /// Total occurrence count across all locations.
    pub total_count: usize,
}

/// Scan all brain files for duplicate lines and blocks.
///
/// Returns a list of duplicate clusters, sorted by total_count descending.
/// Each cluster represents one piece of content that appears multiple times.
pub fn scan_brain_files(brain_path: &Path) -> Vec<DuplicateCluster> {
    let mut line_occurrences: HashMap<String, Vec<(String, usize)>> = HashMap::new();

    for filename in BRAIN_FILES_TO_SCAN {
        let file_path = brain_path.join(filename);
        if !file_path.exists() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&file_path) else {
            continue;
        };

        for (line_idx, line) in content.lines().enumerate() {
            if is_structural_line(line) {
                continue;
            }
            let normalized = line.trim().to_string();
            if normalized.len() < MIN_LINE_LEN {
                continue;
            }
            line_occurrences
                .entry(normalized)
                .or_default()
                .push((filename.to_string(), line_idx + 1));
        }
    }

    // Group into clusters: only keep entries with >= MIN_DUPLICATE_COUNT
    let mut clusters: Vec<DuplicateCluster> = Vec::new();
    for (text, locations) in line_occurrences {
        let total_count: usize = locations.iter().map(|(_, lines)| lines.len()).sum();
        if total_count < MIN_DUPLICATE_COUNT {
            continue;
        }
        // Group by file
        let mut by_file: HashMap<String, Vec<usize>> = HashMap::new();
        for (file, line) in &locations {
            by_file.entry(file.clone()).or_default().push(*line);
        }
        let mut loc_vec: Vec<(String, Vec<usize>)> = by_file.into_iter().collect();
        loc_vec.sort_by(|a, b| a.0.cmp(&b.0));

        clusters.push(DuplicateCluster {
            text,
            locations: loc_vec,
            total_count,
        });
    }

    // Sort by count descending, then by text for stability
    clusters.sort_by(|a, b| b.total_count.cmp(&a.total_count).then(a.text.cmp(&b.text)));
    clusters
}

/// Convert a duplicate cluster into a `ProposedBrainDedup` payload.
///
/// Picks the first file/line as the "canonical" location and proposes
/// removing the duplicates from other locations (or later occurrences
/// in the same file).
pub fn cluster_to_proposal(cluster: &DuplicateCluster) -> Option<ProposedBrainDedup> {
    if cluster.locations.is_empty() {
        return None;
    }

    // Pick the first location as canonical (keep it), remove from others
    let (canonical_file, canonical_lines) = &cluster.locations[0];

    // If there's only one file but multiple lines, remove all but the first
    // If there are multiple files, remove from all files except canonical
    let (target_file, lines_to_remove) = if cluster.locations.len() == 1 {
        // Same file, multiple occurrences
        if canonical_lines.len() <= 1 {
            return None; // Nothing to remove
        }
        (canonical_file.clone(), canonical_lines[1..].to_vec())
    } else {
        // Multiple files — remove from all non-canonical files
        // For simplicity, target the second file in the list
        let (other_file, other_lines) = &cluster.locations[1];
        (other_file.clone(), other_lines.clone())
    };

    if lines_to_remove.is_empty() {
        return None;
    }

    let line_range = if lines_to_remove.len() == 1 {
        format!("{}", lines_to_remove[0])
    } else {
        format!(
            "{}-{}",
            lines_to_remove.iter().min().unwrap(),
            lines_to_remove.iter().max().unwrap()
        )
    };

    let duplicate_of = format!("{}:{}", canonical_file, canonical_lines[0]);

    Some(ProposedBrainDedup {
        target_file,
        duplicate_text: cluster.text.clone(),
        line_range,
        duplicate_of,
        count: cluster.total_count - 1, // subtract the canonical copy we keep
    })
}

/// Run the full scan and return proposals ready for the inbox.
pub fn generate_dedup_proposals(brain_path: &Path) -> Vec<(ProposedBrainDedup, String)> {
    let clusters = scan_brain_files(brain_path);
    let mut results = Vec::new();

    for cluster in &clusters {
        if let Some(proposal) = cluster_to_proposal(cluster) {
            let rationale = format!(
                "Found '{}' appearing {} times across brain files. \
                 Keeping canonical copy at {}, removing duplicate(s).",
                &cluster.text[..cluster.text.len().min(80)],
                cluster.total_count,
                proposal.duplicate_of,
            );
            results.push((proposal, rationale));
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_brain_file(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn test_structural_line_detection() {
        assert!(is_structural_line(""));
        assert!(is_structural_line("# Heading"));
        assert!(is_structural_line("---"));
        assert!(is_structural_line("| --- | --- |"));
        assert!(!is_structural_line(
            "This is a real line with actual content that should be scanned"
        ));
    }

    #[test]
    fn test_scan_finds_duplicates_in_same_file() {
        let dir = TempDir::new().unwrap();
        let content = "# SOUL.md\n\n\
                        Keep responses under 3 sentences when possible.\n\n\
                        Some other content here that is unique.\n\n\
                        Keep responses under 3 sentences when possible.\n\n\
                        More unique content here.\n";
        write_brain_file(dir.path(), "SOUL.md", content);

        let clusters = scan_brain_files(dir.path());
        assert!(!clusters.is_empty(), "Should find duplicate line");

        let dup = &clusters[0];
        assert_eq!(dup.total_count, 2);
        assert!(dup.text.contains("Keep responses under 3 sentences"));
    }

    #[test]
    fn test_scan_finds_duplicates_across_files() {
        let dir = TempDir::new().unwrap();
        let shared_line =
            "Never push tags or releases without EXPLICIT user approval. This is critical.";
        write_brain_file(
            dir.path(),
            "SOUL.md",
            &format!("# SOUL.md\n\n{}\n\nOther soul content.\n", shared_line),
        );
        write_brain_file(
            dir.path(),
            "AGENTS.md",
            &format!("# AGENTS.md\n\n{}\n\nOther agents content.\n", shared_line),
        );

        let clusters = scan_brain_files(dir.path());
        assert!(!clusters.is_empty(), "Should find cross-file duplicate");

        let dup = &clusters[0];
        assert_eq!(dup.total_count, 2);
        assert_eq!(dup.locations.len(), 2);
    }

    #[test]
    fn test_scan_skips_structural_lines() {
        let dir = TempDir::new().unwrap();
        let content = "# SOUL.md\n\n# Heading\n\n# Heading\n\n---\n\n---\n";
        write_brain_file(dir.path(), "SOUL.md", content);

        let clusters = scan_brain_files(dir.path());
        assert!(
            clusters.is_empty(),
            "Should not flag headings or separators as duplicates"
        );
    }

    #[test]
    fn test_cluster_to_proposal_same_file() {
        let cluster = DuplicateCluster {
            text: "Duplicate line content here that is long enough".to_string(),
            locations: vec![("SOUL.md".to_string(), vec![5, 10, 15])],
            total_count: 3,
        };

        let proposal = cluster_to_proposal(&cluster).unwrap();
        assert_eq!(proposal.target_file, "SOUL.md");
        assert_eq!(proposal.duplicate_of, "SOUL.md:5");
        assert_eq!(proposal.count, 2);
        assert!(proposal.line_range.contains("10"));
    }

    #[test]
    fn test_cluster_to_proposal_cross_file() {
        let cluster = DuplicateCluster {
            text: "Shared rule that appears in both files here".to_string(),
            locations: vec![
                ("AGENTS.md".to_string(), vec![20]),
                ("SOUL.md".to_string(), vec![30]),
            ],
            total_count: 2,
        };

        let proposal = cluster_to_proposal(&cluster).unwrap();
        // First location alphabetically is canonical (AGENTS.md)
        // Second (SOUL.md) is the target for removal
        assert_eq!(proposal.target_file, "SOUL.md");
        assert_eq!(proposal.duplicate_of, "AGENTS.md:20");
        assert_eq!(proposal.count, 1);
    }

    #[test]
    fn test_generate_dedup_proposals_end_to_end() {
        let dir = TempDir::new().unwrap();
        let repeated = "Keep responses under 3 sentences when possible.";
        let content = format!(
            "# SOUL.md\n\n{}\n\nUnique content.\n\n{}\n\nMore unique.\n\n{}\n",
            repeated, repeated, repeated
        );
        write_brain_file(dir.path(), "SOUL.md", &content);

        let proposals = generate_dedup_proposals(dir.path());
        assert!(!proposals.is_empty(), "Should generate at least one proposal");

        let (proposal, rationale) = &proposals[0];
        assert_eq!(proposal.target_file, "SOUL.md");
        assert!(rationale.contains("3 times"));
    }

    #[test]
    fn test_no_duplicates_means_no_proposals() {
        let dir = TempDir::new().unwrap();
        let content = "# SOUL.md\n\nEvery line here is unique.\n\nNo repetition at all.\n\nCompletely distinct content.\n";
        write_brain_file(dir.path(), "SOUL.md", content);

        let proposals = generate_dedup_proposals(dir.path());
        assert!(proposals.is_empty(), "Should not generate proposals when no duplicates exist");
    }
}
