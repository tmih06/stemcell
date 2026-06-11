//! Tests for the pure link + anchor resolver.

use crate::brain::kg::resolver::{self, Resolver};

fn sample_resolver() -> Resolver {
    Resolver::from_notes(vec![
        ("concepts/Rust Async.md".to_string(), "Rust Async".to_string()),
        ("concepts/Tokio.md".to_string(), "The Tokio Runtime".to_string()),
        ("people/Alice.md".to_string(), "Alice".to_string()),
    ])
}

#[test]
fn resolves_by_title_case_insensitive() {
    let r = sample_resolver();
    assert_eq!(r.resolve("rust async"), Some("concepts/Rust Async.md"));
    assert_eq!(r.resolve("RUST ASYNC"), Some("concepts/Rust Async.md"));
}

#[test]
fn resolves_by_nested_filename_stem() {
    let r = sample_resolver();
    // Title is "The Tokio Runtime" but the filename stem is "Tokio".
    assert_eq!(r.resolve("Tokio"), Some("concepts/Tokio.md"));
}

#[test]
fn title_preferred_over_stem() {
    // A note whose title collides with another note's stem: title wins.
    let r = Resolver::from_notes(vec![
        ("a/Foo.md".to_string(), "Bar".to_string()),
        ("b/Bar.md".to_string(), "Something Else".to_string()),
    ]);
    // "Bar" matches a/Foo.md by title, not b/Bar.md by stem.
    assert_eq!(r.resolve("Bar"), Some("a/Foo.md"));
}

#[test]
fn dangling_link_resolves_to_none() {
    let r = sample_resolver();
    assert_eq!(r.resolve("Nonexistent Note"), None);
    assert_eq!(r.resolve("   "), None);
}

#[test]
fn filename_stem_strips_folders_and_extension() {
    assert_eq!(
        resolver::filename_stem("concepts/Rust Async.md").as_deref(),
        Some("Rust Async")
    );
    assert_eq!(resolver::filename_stem("Top.md").as_deref(), Some("Top"));
}

const DOC: &str = "# Title\nintro\n## A\na body\n### A1\na1 body\n## B\nb body\n";

#[test]
fn heading_range_covers_section_and_subsections() {
    // ## A spans through ### A1 until ## B.
    let range = resolver::heading_range(DOC, "A").expect("range");
    let sliced = resolver::slice_lines(DOC, range);
    assert!(sliced.starts_with("## A"));
    assert!(sliced.contains("### A1"));
    assert!(sliced.contains("a1 body"));
    assert!(!sliced.contains("## B"));
}

#[test]
fn heading_range_subsection_stops_at_higher_level() {
    let range = resolver::heading_range(DOC, "A1").expect("range");
    let sliced = resolver::slice_lines(DOC, range);
    assert!(sliced.starts_with("### A1"));
    assert!(sliced.contains("a1 body"));
    assert!(!sliced.contains("## B"));
}

#[test]
fn heading_range_last_section_runs_to_eof() {
    let range = resolver::heading_range(DOC, "B").expect("range");
    let sliced = resolver::slice_lines(DOC, range);
    assert!(sliced.starts_with("## B"));
    assert!(sliced.contains("b body"));
}

#[test]
fn heading_range_missing_returns_none() {
    assert!(resolver::heading_range(DOC, "Nope").is_none());
}

#[test]
fn block_range_expands_to_paragraph() {
    let content = "para line 1\npara line 2 ^myblock\n\nother\n";
    let range = resolver::block_range(content, "myblock").expect("range");
    let sliced = resolver::slice_lines(content, range);
    assert!(sliced.contains("para line 1"));
    assert!(sliced.contains("para line 2"));
    assert!(!sliced.contains("other"));
}

#[test]
fn anchor_range_prefers_block_over_heading() {
    let content = "## A\nintro ^b1\n## B\n";
    let by_block = resolver::anchor_range(content, Some("A"), Some("b1")).expect("range");
    let sliced = resolver::slice_lines(content, by_block);
    assert!(sliced.contains("intro"));
    assert!(!sliced.starts_with("## A"));
}
