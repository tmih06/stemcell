//! Tests for the per-file line cap in `sync_single_file`.
//!
//! Issue #164 fix 2: `sync_templates` had no upper bound on how large a
//! brain file could grow. A user-pruned file got re-grown by upstream
//! syncs without any warning. Now: count the merged line total, bail if
//! it exceeds the per-file cap (or the default cap), and surface the
//! bail with a structured `CapBailReport` plus an entry in
//! `~/.opencrabs/rsi/improvements.md` so the user can act on it.
//!
//! We can't easily exercise `sync_single_file` directly here (it does
//! network I/O for the upstream template fetch and writes to the user's
//! actual brain dir), so the tests pin two layers:
//!
//! 1. `BrainConfig::cap_for` — the resolver that picks per-file vs
//!    default cap from config.
//! 2. Source-level invariants in `rsi_sync.rs` — the cap check is wired
//!    into `sync_single_file`, the bail surfaces a `CapBailReport`, and
//!    the structured report's fields are populated.

use crate::config::BrainConfig;
use std::collections::BTreeMap;

#[test]
fn cap_for_returns_per_file_when_listed() {
    let mut caps = BTreeMap::new();
    caps.insert("TOOLS.md".to_string(), 800);
    caps.insert("MEMORY.md".to_string(), 200);
    let cfg = BrainConfig {
        caps,
        ..Default::default()
    };
    assert_eq!(cfg.cap_for("TOOLS.md"), 800);
    assert_eq!(cfg.cap_for("MEMORY.md"), 200);
}

#[test]
fn cap_for_falls_back_to_default_when_unlisted() {
    let mut caps = BTreeMap::new();
    caps.insert("TOOLS.md".to_string(), 800);
    let cfg = BrainConfig {
        default_cap: 600,
        caps,
        ..Default::default()
    };
    assert_eq!(
        cfg.cap_for("AGENTS.md"),
        600,
        "unlisted files must use default_cap, not 0 or a per-file default"
    );
}

#[test]
fn default_cap_matches_issue_spec_of_500() {
    let cfg = BrainConfig::default();
    assert_eq!(
        cfg.default_cap, 500,
        "issue #164 specified 500 as the default; raising it later would \
         silently widen the cap for every existing config without per-file \
         overrides — change this test deliberately if the policy moves"
    );
    assert_eq!(cfg.cap_for("anything.md"), 500);
}

#[test]
fn caps_map_serializes_as_empty_btreemap_by_default() {
    let cfg = BrainConfig::default();
    assert!(cfg.caps.is_empty());
    let _: &BTreeMap<String, usize> = &cfg.caps; // type check
}

#[test]
fn case_sensitive_filename_lookup() {
    let mut caps = BTreeMap::new();
    caps.insert("TOOLS.md".to_string(), 800);
    let cfg = BrainConfig {
        caps,
        ..Default::default()
    };
    let default = cfg.default_cap;
    assert_eq!(cfg.cap_for("tools.md"), default);
    assert_eq!(cfg.cap_for("TOOLS.md"), 800);
}

// ── Source-level invariants on sync_single_file ──────────────────────

const RSI_SYNC_SRC: &str = include_str!("../brain/rsi_sync.rs");

/// Strip line comments so source-level scans don't false-match against
/// regression docs that name the bug.
fn rsi_sync_src_code() -> String {
    RSI_SYNC_SRC
        .lines()
        .map(|line| {
            if let Some(idx) = line.find("//") {
                let before = &line[..idx];
                if before.matches('"').count() % 2 == 0 {
                    return before.trim_end().to_string();
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn sync_single_file_consults_brain_caps_before_writing() {
    let src = rsi_sync_src_code();
    // The cap check must read from BrainConfig and call cap_for(filename).
    // A regression that drops the lookup and writes unconditionally would
    // re-introduce the unbounded-growth bug.
    assert!(
        src.contains("brain_cfg.cap_for(filename)"),
        "sync_single_file must resolve cap via BrainConfig::cap_for(filename) \
         before deciding to write — without it, the merged file can grow \
         past the user's budget silently."
    );
    assert!(
        src.contains("merged_line_count > cap"),
        "sync_single_file must compare merged line count against the cap \
         and bail when it exceeds."
    );
}

#[test]
fn cap_bail_returns_structured_report() {
    let src = rsi_sync_src_code();
    // The bail path must populate a CapBailReport with the four core
    // fields named in the issue (local_lines, upstream_lines, merged_lines,
    // cap) so Mission Control can render the situation without re-deriving.
    for field in [
        "local_lines:",
        "upstream_lines:",
        "merged_lines:",
        "cap:",
        "top_new_sections:",
    ] {
        assert!(
            src.contains(field),
            "CapBailReport must populate `{field}` — Mission Control / improvements.md \
             rendering depends on every field being present at construction time."
        );
    }
    assert!(
        src.contains("bailed_for_cap: Some(report)"),
        "sync_single_file's bail path must return FileSyncResult with \
         bailed_for_cap=Some(...) so the caller can distinguish cap-bail \
         from transient I/O errors."
    );
}

#[test]
fn cap_bail_logs_to_improvements_md() {
    let src = rsi_sync_src_code();
    assert!(
        src.contains("log_cap_bail_to_improvements"),
        "cap-bail must persist a diagnostic entry to ~/.opencrabs/rsi/improvements.md \
         so the user sees the bail next session without scraping stdout."
    );
    // The log function itself must mention improvements.md to confirm
    // the target file hasn't drifted.
    assert!(
        src.contains("improvements.md"),
        "log_cap_bail_to_improvements must write to improvements.md by name"
    );
}

#[test]
fn top_new_sections_helper_caps_at_n_largest() {
    use crate::brain::rsi_sync::top_new_sections_by_size_for_test;

    // Big = 6 body lines, Medium = 4 body lines, Small = 1 body line.
    // Distinct counts so the ranking is unambiguous (no tie-break race).
    let new_sections =
        "\n## Big\nb1\nb2\nb3\nb4\nb5\nb6\n\n## Small\nsmall body\n\n## Medium\nm1\nm2\nm3\nm4\n";
    let top = top_new_sections_by_size_for_test(new_sections, 2);
    assert_eq!(
        top.len(),
        2,
        "must respect the N cap, not flood the report with every section"
    );
    assert!(
        top[0].starts_with("## Big"),
        "largest section must rank first; got {:?}",
        top
    );
    assert!(
        top[1].starts_with("## Medium"),
        "second-largest must rank second; got {:?}",
        top
    );
}
