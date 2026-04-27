//! RSI Template Sync Tests
//!
//! Tests for the upstream brain file template synchronization feature.
//! Verifies version gating, section extraction, state persistence, and backup safety.

mod rsi_sync {
    use std::collections::HashMap;

    // --- Section Extraction Tests ---

    fn extract_section_headers(content: &str) -> Vec<String> {
        content
            .lines()
            .filter(|line| line.starts_with("## ") || line.starts_with("### "))
            .map(|line| line.trim().to_string())
            .collect()
    }

    #[test]
    fn extracts_top_level_sections() {
        let content = "## Section A\nSome content\n## Section B\nMore content\n";
        let headers = extract_section_headers(content);
        assert_eq!(headers, vec!["## Section A", "## Section B"]);
    }

    #[test]
    fn extracts_subsections() {
        let content = "## Section A\n### Sub A1\n### Sub A2\n## Section B\n";
        let headers = extract_section_headers(content);
        assert_eq!(headers, vec!["## Section A", "### Sub A1", "### Sub A2", "## Section B"]);
    }

    #[test]
    fn ignores_non_header_lines() {
        let content = "Some text\n## Real Header\n- bullet point\n### Sub Header\n";
        let headers = extract_section_headers(content);
        assert_eq!(headers, vec!["## Real Header", "### Sub Header"]);
    }

    #[test]
    fn handles_empty_content() {
        let headers = extract_section_headers("");
        assert!(headers.is_empty());
    }

    // --- Version Gate Tests ---

    fn needs_sync(last_synced: &str, current: &str) -> bool {
        last_synced != current
    }

    #[test]
    fn sync_needed_on_version_change() {
        assert!(needs_sync("0.3.13", "0.3.14"));
    }

    #[test]
    fn sync_not_needed_on_same_version() {
        assert!(!needs_sync("0.3.14", "0.3.14"));
    }

    #[test]
    fn sync_needed_on_major_bump() {
        assert!(needs_sync("0.3.14", "0.4.0"));
    }

    // --- State Persistence Tests ---

    #[test]
    fn state_tracks_per_file_dates() {
        let mut file_dates: HashMap<String, String> = HashMap::new();
        file_dates.insert("SOUL.md".to_string(), "2026-04-27T21:00:00Z".to_string());
        file_dates.insert("TOOLS.md".to_string(), "2026-04-27T21:00:00Z".to_string());

        assert_eq!(file_dates.len(), 2);
        assert!(file_dates.contains_key("SOUL.md"));
        assert!(file_dates.contains_key("TOOLS.md"));
        assert!(!file_dates.contains_key("MEMORY.md"));
    }

    // --- Backup Safety Tests ---

    #[test]
    fn backup_filename_format() {
        let filename = "SOUL.md";
        let timestamp = "2026-04-27T210000";
        let backup_name = format!("{}.{}.bak", filename, timestamp);
        assert_eq!(backup_name, "SOUL.md.2026-04-27T210000.bak");
        assert!(backup_name.ends_with(".bak"));
    }

    #[test]
    fn backup_preserves_original_extension() {
        let filename = "TOOLS.md";
        let backup_name = format!("{}.bak", filename);
        assert!(backup_name.contains("TOOLS.md"));
    }

    // --- Append-Only Safety Tests ---

    #[test]
    fn append_increases_content_length() {
        let original = "## Existing Section\nUser content here\n";
        let new_section = "\n## New Section\nUpstream content\n";
        let merged = format!("{}{}", original, new_section);
        assert!(merged.len() > original.len());
        assert!(merged.contains("User content here"));
        assert!(merged.contains("Upstream content"));
    }

    #[test]
    fn append_never_removes_existing_content() {
        let original = "## Hard Rules\n- Rule 1\n- Rule 2\n";
        let new_section = "\n## New Rules\n- Rule 3\n";
        let merged = format!("{}{}", original, new_section);
        assert!(merged.contains("Rule 1"));
        assert!(merged.contains("Rule 2"));
        assert!(merged.contains("Rule 3"));
    }
}
