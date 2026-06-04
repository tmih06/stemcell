//! Tests for `brain::filter::strip_empty_sections`.
//!
//! Issue #164 fix 4: header stubs (sections whose body is empty after a
//! manual prune or dedup pass) bloat the LLM's view of brain context
//! without adding signal. The filter scrubs them at READ time, leaving
//! disk authoritative.

use crate::brain::filter::{strip_empty_sections, StripResult};

#[test]
fn empty_stub_is_stripped() {
    let input = "# Title\n\n## Real\nbody text here\n\n## Empty\n\n## Next Real\nmore body\n";
    let res = strip_empty_sections(input);
    assert!(
        !res.content.contains("## Empty"),
        "header with no body must be removed; got:\n{}",
        res.content
    );
    assert!(res.content.contains("## Real"));
    assert!(res.content.contains("## Next Real"));
    assert_eq!(res.stripped_headers, vec!["## Empty".to_string()]);
}

#[test]
fn header_with_body_is_kept() {
    let input = "## Has Body\nsome content here\n";
    let res = strip_empty_sections(input);
    assert_eq!(
        res.content, input,
        "non-empty body must round-trip unchanged"
    );
    assert!(res.stripped_headers.is_empty());
}

#[test]
fn tbd_marker_keeps_section_alive() {
    let input = "## In Progress\nTBD\n";
    let res = strip_empty_sections(input);
    assert_eq!(
        res.content, input,
        "TBD body must survive — it's an intentional in-flight marker"
    );
    assert!(res.stripped_headers.is_empty());
}

#[test]
fn todo_wip_placeholder_all_keep_section_alive() {
    for marker in ["TODO: write this", "WIP work in progress", "placeholder line"] {
        let input = format!("## Marked\n{}\n", marker);
        let res = strip_empty_sections(&input);
        assert_eq!(
            res.content, input,
            "in-flight marker '{}' must keep section alive",
            marker
        );
    }
}

#[test]
fn marker_must_be_whole_word_not_substring() {
    // A body that is otherwise structural-only ("<!-- ... -->") but
    // contains TODO embedded in a larger word. A correct word-boundary
    // check sees the comment as structural-only with no real marker
    // and strips the section. A buggy substring check would treat
    // "TODOPHONE" as a TODO marker, protect the section, and skip the
    // strip — which is the exact false-positive we're guarding against.
    let input = "## Stub\n<!-- TODOPHONE -->\n";
    let res = strip_empty_sections(input);
    assert!(
        !res.content.contains("## Stub"),
        "substring 'TODO' inside 'TODOPHONE' must NOT count as an \
         in-flight marker — only whole-word matches protect a section. \
         Got content:\n{}",
        res.content
    );
}

#[test]
fn structural_only_body_is_stripped() {
    // Horizontal rule alone, table separator alone, HTML comment alone,
    // short blockquote alone — all count as empty.
    let cases = [
        "## HR\n---\n",
        "## Table\n| --- | --- |\n",
        "## Comment\n<!-- a note -->\n",
        "## Quote\n> short\n",
    ];
    for input in cases {
        let res = strip_empty_sections(input);
        assert!(
            res.stripped_headers.len() == 1,
            "structural-only body must strip: {:?} -> {:?}",
            input,
            res
        );
    }
}

#[test]
fn nested_subsections_handled_independently() {
    // `### Empty` under `## Outer` should strip without affecting Outer.
    // `## Outer` itself keeps its non-subheader body.
    let input = "## Outer\nreal outer body\n\n### Empty Sub\n\n### Real Sub\nsub body\n";
    let res = strip_empty_sections(input);
    assert!(!res.content.contains("### Empty Sub"));
    assert!(res.content.contains("## Outer"));
    assert!(res.content.contains("### Real Sub"));
    assert_eq!(res.stripped_headers, vec!["### Empty Sub".to_string()]);
}

#[test]
fn empty_outer_with_real_subsection_is_kept() {
    // `## Outer` has no body of its own but contains `### Real Sub` with
    // content. Outer must survive because its body region (until next
    // same-or-higher header) contains a non-empty subsection.
    let input = "## Outer\n\n### Real Sub\nsub body\n";
    let res = strip_empty_sections(input);
    assert!(
        res.content.contains("## Outer"),
        "outer header with non-empty nested subsection must survive"
    );
    assert!(res.stripped_headers.is_empty());
}

#[test]
fn level_1_headers_never_stripped() {
    // `# Title` is the document title — we never touch level 1 regardless
    // of body emptiness. Avoids accidentally wiping a SOUL.md or AGENTS.md
    // top header on a freshly-curated file.
    let input = "# SOUL\n\n## Real\nbody\n";
    let res = strip_empty_sections(input);
    assert!(res.content.starts_with("# SOUL"));
}

#[test]
fn trailing_newline_preserved() {
    let input = "## Real\nbody\n";
    let res = strip_empty_sections(input);
    assert!(res.content.ends_with('\n'));
}

#[test]
fn trailing_newline_absent_stays_absent() {
    let input = "## Real\nbody";
    let res = strip_empty_sections(input);
    assert!(!res.content.ends_with('\n'));
}

#[test]
fn empty_input_returns_default() {
    let res = strip_empty_sections("");
    assert_eq!(res, StripResult::default());
}

#[test]
fn multiple_consecutive_empty_stubs_all_stripped() {
    let input = "## A\n\n## B\n\n## C\nbody\n";
    let res = strip_empty_sections(input);
    assert!(!res.content.contains("## A"));
    assert!(!res.content.contains("## B"));
    assert!(res.content.contains("## C"));
    assert_eq!(res.stripped_headers.len(), 2);
}

#[test]
fn no_op_when_nothing_to_strip() {
    let input = "## A\nbody a\n\n## B\nbody b\n";
    let res = strip_empty_sections(input);
    assert_eq!(res.content, input);
    assert!(res.stripped_headers.is_empty());
}

#[test]
fn header_followed_immediately_by_eof_is_stripped() {
    let input = "## Lonely";
    let res = strip_empty_sections(input);
    assert_eq!(res.content, "");
    assert_eq!(res.stripped_headers, vec!["## Lonely".to_string()]);
}
