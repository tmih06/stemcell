//! Tests for the pure markdown-bullet insertion helpers in `kg_note`.
//!
//! These cover the idempotent-append behavior: re-remembering a fact must not
//! duplicate a bullet, and the inserted-count must reflect only genuinely-new
//! bullets so the success message doesn't overstate what was added.

use crate::brain::tools::kg_note::{
    build_note, insert_bullets, observation_bullet, relation_bullet,
};
use serde_json::json;

const NOTE: &str = "---\n\
title: Rust Async\n\
type: concept\n\
created: 2026-06-11\n\
---\n\
\n\
# Rust Async\n\
\n\
## Observations\n\
- [fact] Futures are lazy #rust\n\
\n\
## Relations\n\
- depends_on [[Tokio Runtime]]\n";

#[test]
fn inserting_existing_bullet_is_a_noop() {
    let bullet = observation_bullet("[fact] Futures are lazy #rust");
    let (out, inserted) = insert_bullets(NOTE, "Observations", &[bullet]);
    assert_eq!(inserted, 0, "existing bullet should not be re-added");
    assert_eq!(out, NOTE, "content must be unchanged on full duplicate");
    assert_eq!(
        out.matches("Futures are lazy").count(),
        1,
        "no duplicate line should appear"
    );
}

#[test]
fn inserting_mix_only_adds_new() {
    let dup = observation_bullet("[fact] Futures are lazy #rust");
    let fresh = observation_bullet("[fact] Tasks are spawned onto an executor");
    let (out, inserted) = insert_bullets(NOTE, "Observations", &[dup, fresh]);
    assert_eq!(inserted, 1, "only the genuinely-new bullet counts");
    assert_eq!(out.matches("Futures are lazy").count(), 1);
    assert_eq!(out.matches("Tasks are spawned").count(), 1);
    // The new bullet lands inside Observations, before the Relations heading.
    let obs_idx = out.find("Tasks are spawned").unwrap();
    let rel_idx = out.find("## Relations").unwrap();
    assert!(obs_idx < rel_idx, "new bullet must stay in its section");
}

#[test]
fn inserting_when_section_absent_creates_it() {
    // NOTE has no "Tags" section; inserting should append a fresh one.
    let bullet = "- #rust".to_string();
    let (out, inserted) = insert_bullets(NOTE, "Tags", &[bullet]);
    assert_eq!(inserted, 1);
    assert!(out.contains("## Tags"), "missing section must be created");
    assert!(out.contains("- #rust"));
    assert!(out.ends_with('\n'), "trailing newline guarantee preserved");
}

#[test]
fn multiple_new_bullets_all_added() {
    let a = observation_bullet("[fact] one");
    let b = observation_bullet("[fact] two");
    let (out, inserted) = insert_bullets(NOTE, "Observations", &[a, b]);
    assert_eq!(inserted, 2);
    assert!(out.contains("- [fact] one"));
    assert!(out.contains("- [fact] two"));
}

#[test]
fn relation_dedup_against_existing() {
    let rel = relation_bullet(&json!({"type": "depends_on", "target": "Tokio Runtime"})).unwrap();
    let (out, inserted) = insert_bullets(NOTE, "Relations", &[rel]);
    assert_eq!(inserted, 0, "identical relation must not duplicate");
    assert_eq!(out.matches("Tokio Runtime").count(), 1);
}

#[test]
fn build_note_inserts_all_bullets() {
    let obs = vec![observation_bullet("[fact] alpha")];
    let rels = vec![relation_bullet(&json!({"target": "Beta"})).unwrap()];
    let note = build_note("Alpha", Some("concept"), &obs, &rels);
    assert!(note.contains("# Alpha"));
    assert!(note.contains("## Observations"));
    assert!(note.contains("- [fact] alpha"));
    assert!(note.contains("## Relations"));
    assert!(note.contains("- [[Beta]]"));
    assert!(note.ends_with('\n'));
}
