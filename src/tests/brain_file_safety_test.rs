//! Tests for `brain_file_safety` — the chokepoint that protects
//! `~/.opencrabs/*.md` brain files from agent-driven destruction.
//!
//! Background: 2026-04-26 the RSI agent shrank TOOLS.md from 33 KB to a
//! stub by passing the entire file as `old_content` to a
//! `self_improve` action="update". MEMORY.md got the same treatment.
//! The user policy is append-only — removals only when the bytes
//! disappearing are duplicates that survive elsewhere in the result.

use crate::brain::tools::brain_file_safety::{
    ShrinkCheck, backup_before_write, check_no_shrink, is_protected_brain_file, is_protected_path,
};
use std::path::Path;

mod protected_predicate {
    use super::*;

    #[test]
    fn known_brain_files_are_protected() {
        for name in [
            "SOUL.md",
            "USER.md",
            "AGENTS.md",
            "TOOLS.md",
            "CODE.md",
            "SECURITY.md",
            "MEMORY.md",
            "BOOT.md",
            "IDENTITY.md",
        ] {
            assert!(is_protected_brain_file(name), "{name} should be protected");
        }
    }

    #[test]
    fn case_insensitive_match() {
        // The agent sometimes lowercases names — protect either way.
        assert!(is_protected_brain_file("memory.md"));
        assert!(is_protected_brain_file("tools.md"));
    }

    #[test]
    fn unrelated_files_are_not_protected() {
        assert!(!is_protected_brain_file("commands.toml"));
        assert!(!is_protected_brain_file("notes.md"));
        assert!(!is_protected_brain_file("memory/2026-04-26.md"));
    }

    #[test]
    fn path_predicate_uses_basename() {
        assert!(is_protected_path(Path::new("/Users/x/.opencrabs/TOOLS.md")));
        assert!(is_protected_path(Path::new("TOOLS.md")));
        assert!(!is_protected_path(Path::new(
            "/Users/x/.opencrabs/memory/log.md"
        )));
    }
}

mod shrink_check {
    use super::*;

    fn protected() -> &'static Path {
        Path::new("/tmp/fake/TOOLS.md")
    }

    fn unprotected() -> &'static Path {
        Path::new("/tmp/fake/notes.md")
    }

    #[test]
    fn unprotected_file_is_always_allowed() {
        // Non-brain files can shrink freely — that's not what this guard
        // is for.
        let result = check_no_shrink(unprotected(), "lots of content here", "tiny", false);
        assert_eq!(result, ShrinkCheck::Allowed);
    }

    #[test]
    fn append_to_protected_is_allowed() {
        // The append-only happy path — content grows.
        let existing = "line one\nline two\n";
        let updated = "line one\nline two\nline three\n";
        assert_eq!(
            check_no_shrink(protected(), existing, updated, false),
            ShrinkCheck::Allowed
        );
    }

    #[test]
    fn equal_size_rewrite_is_allowed() {
        // Same length update (e.g. typo fix) doesn't shrink, so it's
        // allowed even on a protected file.
        let existing = "abc def ghi";
        let updated = "abc XYZ ghi";
        assert_eq!(
            check_no_shrink(protected(), existing, updated, false),
            ShrinkCheck::Allowed
        );
    }

    #[test]
    fn shrink_without_dedup_intent_is_rejected() {
        // The 2026-04-26 case: agent rewrites the whole file from many
        // KB down to a stub. Hard reject.
        let existing = "rule one\nrule two\nrule three\nrule four\nrule five\n";
        let updated = "rule one\n";
        match check_no_shrink(protected(), existing, updated, false) {
            ShrinkCheck::Rejected { message } => {
                assert!(message.contains("Refusing to shrink"));
                assert!(message.contains("TOOLS.md"));
                assert!(message.contains("append-only"));
            }
            other => panic!("expected Rejected, got {:?}", other),
        }
    }

    #[test]
    fn shrink_with_dedup_intent_passes_when_lines_survive() {
        // Legit dedup: existing has the same line twice, updated has it
        // once. Every original line is still present somewhere.
        let existing = "alpha\nbeta\nalpha\ngamma\n";
        let updated = "alpha\nbeta\ngamma\n";
        assert_eq!(
            check_no_shrink(protected(), existing, updated, true),
            ShrinkCheck::Allowed
        );
    }

    #[test]
    fn shrink_with_dedup_intent_rejected_when_unique_line_disappears() {
        // dedup_intent is set but the agent is sneakily removing
        // unique content. The hint mentions the intent specifically
        // so the agent learns it can't be used as a bypass.
        let existing = "rule one\nrule two\nrule three\n";
        let updated = "rule one\n";
        match check_no_shrink(protected(), existing, updated, true) {
            ShrinkCheck::Rejected { message } => {
                assert!(
                    message.contains("dedup_intent"),
                    "rejection should call out the abused dedup_intent flag: {message}"
                );
            }
            other => panic!("expected Rejected with dedup_intent hint, got {:?}", other),
        }
    }

    #[test]
    fn empty_lines_in_original_dont_block_dedup() {
        // Blank-line spacing in the original shouldn't be required to
        // round-trip — only non-blank lines count toward the survival
        // check.
        let existing = "alpha\n\nbeta\n\nalpha\n";
        let updated = "alpha\nbeta\n";
        assert_eq!(
            check_no_shrink(protected(), existing, updated, true),
            ShrinkCheck::Allowed
        );
    }
}

mod backup {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn skips_backup_for_nonexistent_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("doesnotexist.md");
        let result = backup_before_write(&path).expect("ok");
        assert!(result.is_none());
    }

    #[test]
    fn backs_up_existing_file_to_timestamped_path() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("TOOLS.md");
        std::fs::write(&path, "important content").expect("seed");

        let backup = backup_before_write(&path)
            .expect("ok")
            .expect("backup path returned");

        assert!(backup.exists(), "backup file should exist");
        let backup_name = backup.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            backup_name.starts_with("TOOLS.md."),
            "backup name should start with original: {backup_name}"
        );
        assert!(
            backup_name.ends_with(".bak"),
            "backup name should end with .bak: {backup_name}"
        );
        let backed_up = std::fs::read_to_string(&backup).expect("read backup");
        assert_eq!(backed_up, "important content");

        // Original is untouched
        let original = std::fs::read_to_string(&path).expect("read original");
        assert_eq!(original, "important content");
    }
}
