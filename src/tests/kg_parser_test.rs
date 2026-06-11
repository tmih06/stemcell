//! Tests for the pure markdown knowledge-graph parser.

use crate::brain::kg::parser::{self, WikiLink};

const SAMPLE: &str = r#"---
title: Rust Async
type: concept
tags: [rust, concurrency]
aliases: [async rust, tokio model]
created: 2026-06-11
---

# Rust Async

## Observations
- [fact] Futures are lazy; nothing runs until polled #rust
- [gotcha] Holding a std Mutex across .await deadlocks (deadlock risk)

## Relations
- depends_on [[Tokio Runtime]]
- contrasts_with [[Thread-per-request]]
- [[Pinning]]
"#;

#[test]
fn parses_frontmatter_fields() {
    let note = parser::parse(SAMPLE);
    assert_eq!(note.frontmatter.title.as_deref(), Some("Rust Async"));
    assert_eq!(note.frontmatter.note_type.as_deref(), Some("concept"));
    assert_eq!(note.frontmatter.tags, vec!["rust", "concurrency"]);
    assert_eq!(note.frontmatter.aliases, vec!["async rust", "tokio model"]);
    // Unknown keys are preserved in the raw field map.
    assert_eq!(
        note.frontmatter.fields.get("created").and_then(|v| v.as_str()),
        Some("2026-06-11")
    );
    // JSON serialization round-trips the raw fields.
    let json = note.frontmatter.to_json().expect("json");
    assert!(json.contains("\"title\""));
}

#[test]
fn title_prefers_frontmatter_then_h1() {
    let note = parser::parse(SAMPLE);
    assert_eq!(note.title.as_deref(), Some("Rust Async"));

    let no_fm = "# Heading Title\n\nbody";
    assert_eq!(parser::parse(no_fm).title.as_deref(), Some("Heading Title"));

    let neither = "just text, no heading";
    assert_eq!(parser::parse(neither).title, None);
}

#[test]
fn parses_typed_observations() {
    let note = parser::parse(SAMPLE);
    assert_eq!(note.observations.len(), 2);

    let fact = &note.observations[0];
    assert_eq!(fact.category.as_deref(), Some("fact"));
    assert_eq!(fact.content, "Futures are lazy; nothing runs until polled");
    assert_eq!(fact.tags, vec!["rust"]);
    assert_eq!(fact.context, None);

    let gotcha = &note.observations[1];
    assert_eq!(gotcha.category.as_deref(), Some("gotcha"));
    assert_eq!(gotcha.content, "Holding a std Mutex across .await deadlocks");
    assert_eq!(gotcha.context.as_deref(), Some("deadlock risk"));
}

#[test]
fn parses_typed_and_bare_relations() {
    let note = parser::parse(SAMPLE);
    let by_target = |t: &str| note.relations.iter().find(|r| r.target == t).cloned();

    let dep = by_target("Tokio Runtime").expect("tokio rel");
    assert_eq!(dep.relation_type, "depends_on");

    let con = by_target("Thread-per-request").expect("contrast rel");
    assert_eq!(con.relation_type, "contrasts_with");

    let bare = by_target("Pinning").expect("bare rel");
    assert_eq!(bare.relation_type, "links_to");

    // Exactly three relations — no double-counting of section links.
    assert_eq!(note.relations.len(), 3);
}

#[test]
fn parses_wikilink_variants() {
    let line = "See [[Note A]], [[Note B|alias]], [[Note C#Heading]], [[Note D#^block1]] and ![[Embed E]]";
    let links = parser::scan_wikilinks(line);
    assert_eq!(links.len(), 5);

    assert_eq!(
        links[0],
        WikiLink {
            target: "Note A".into(),
            heading: None,
            block_id: None,
            alias: None,
            embed: false
        }
    );
    assert_eq!(links[1].target, "Note B");
    assert_eq!(links[1].alias.as_deref(), Some("alias"));
    assert_eq!(links[2].target, "Note C");
    assert_eq!(links[2].heading.as_deref(), Some("Heading"));
    assert_eq!(links[3].target, "Note D");
    assert_eq!(links[3].block_id.as_deref(), Some("block1"));
    assert!(links[4].embed);
    assert_eq!(links[4].target, "Embed E");
}

#[test]
fn prose_links_fold_into_links_to_without_duplicating_typed() {
    let content = "# N\n\n## Relations\n- depends_on [[A]]\n\n## Notes\nSee also [[B]] and [[A]] again.\n";
    let note = parser::parse(content);
    // A keeps its typed relation; B becomes a links_to; A is not duplicated.
    let a = note.relations.iter().filter(|r| r.target == "A").count();
    let b = note.relations.iter().filter(|r| r.target == "B").count();
    assert_eq!(a, 1, "A should not be double-counted");
    assert_eq!(b, 1);
    let a_rel = note.relations.iter().find(|r| r.target == "A").unwrap();
    assert_eq!(a_rel.relation_type, "depends_on");
    let b_rel = note.relations.iter().find(|r| r.target == "B").unwrap();
    assert_eq!(b_rel.relation_type, "links_to");
}

#[test]
fn merges_inline_and_frontmatter_tags() {
    let note = parser::parse(SAMPLE);
    assert!(note.tags.contains(&"rust".to_string()));
    assert!(note.tags.contains(&"concurrency".to_string()));
}

#[test]
fn collects_headings_and_block_ids() {
    let content = "# Title\n\n## Section\nA paragraph that ends with an anchor. ^para1\n";
    let note = parser::parse(content);
    assert_eq!(note.headings.len(), 2);
    assert_eq!(note.headings[0].level, 1);
    assert_eq!(note.headings[1].text, "Section");
    assert_eq!(note.block_ids.len(), 1);
    assert_eq!(note.block_ids[0].0, "para1");
}

#[test]
fn handles_block_list_frontmatter() {
    let content = "---\ntitle: X\naliases:\n  - alpha\n  - beta\n---\n\n# X\n";
    let note = parser::parse(content);
    assert_eq!(note.frontmatter.aliases, vec!["alpha", "beta"]);
}

#[test]
fn no_frontmatter_is_fine() {
    let content = "# Plain\n\nNo frontmatter here.\n";
    let note = parser::parse(content);
    assert!(note.frontmatter.title.is_none());
    assert_eq!(note.title.as_deref(), Some("Plain"));
    assert!(note.frontmatter.to_json().is_none());
}

#[test]
fn heading_run_without_space_is_not_a_heading() {
    // "#rust" is a tag, not a heading.
    assert_eq!(parser::parse_heading("#rust"), None);
    assert_eq!(parser::parse_heading("## Real"), Some((2, "Real".to_string())));
}
