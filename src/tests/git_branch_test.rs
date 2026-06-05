//! Tests for `utils::git_branch::parse_head` — the pure HEAD-content
//! parser. The walk-up filesystem search (`find_head`) is exercised
//! implicitly by the TUI footer rendering at runtime; pinning the parser
//! here covers the ref-shape branches without filesystem fixtures.
//!
//! Used by the TUI status bar to render `~/path/to/repo (branch)`. A
//! regression that miscategorises detached HEAD or an unusual ref shape
//! would silently break the footer for some users without breaking it
//! for others (the common case stays "ref: refs/heads/main").

use crate::utils::git_branch::parse_head;

#[test]
fn parses_standard_branch_ref() {
    assert_eq!(
        parse_head("ref: refs/heads/main\n").as_deref(),
        Some("main")
    );
    assert_eq!(
        parse_head("ref: refs/heads/feat/new-thing\n").as_deref(),
        Some("feat/new-thing"),
        "branch names with slashes must round-trip in full, not be truncated to the last segment"
    );
}

#[test]
fn parses_branch_ref_without_trailing_newline() {
    assert_eq!(parse_head("ref: refs/heads/dev").as_deref(), Some("dev"));
}

#[test]
fn detached_head_returns_short_sha() {
    let sha = "deadbeefcafef00d1234567890abcdef12345678";
    let parsed = parse_head(sha).expect("detached HEAD must parse to a short SHA");
    assert_eq!(parsed.len(), 7);
    assert_eq!(parsed, "deadbee");
}

#[test]
fn detached_head_with_trailing_newline_still_parses() {
    let sha = "abcdef0123456789abcdef0123456789abcdef01\n";
    assert_eq!(parse_head(sha).as_deref(), Some("abcdef0"));
}

#[test]
fn non_branch_ref_returns_last_path_component() {
    // Hand-curated edge case: `ref: refs/tags/v1.0` (some tooling
    // checks out tags directly). Fall back to the last segment so the
    // footer shows `v1.0` rather than the full ref path.
    assert_eq!(parse_head("ref: refs/tags/v1.0\n").as_deref(), Some("v1.0"));
    assert_eq!(
        parse_head("ref: refs/remotes/origin/main\n").as_deref(),
        Some("main")
    );
}

#[test]
fn empty_input_returns_none() {
    assert_eq!(parse_head(""), None);
    assert_eq!(parse_head("\n"), None);
    assert_eq!(parse_head("   \n  "), None);
}

#[test]
fn garbage_content_returns_none() {
    // Not a ref line, not a hex SHA — must NOT be rendered as a branch.
    // A regression that "falls through" to some default would put junk
    // in the user's footer.
    assert_eq!(parse_head("garbage data here").as_deref(), None);
    assert_eq!(parse_head("ref: ").as_deref(), None);
    // Mixed hex + non-hex
    assert_eq!(parse_head("deadbeefXYZ").as_deref(), None);
}

#[test]
fn ref_with_just_prefix_returns_none() {
    // `ref: refs/heads/` with no branch name is malformed.
    assert_eq!(
        parse_head("ref: refs/heads/").as_deref(),
        Some(""),
        "empty-name ref is parsed as empty string; renderer treats this \
         as a present branch — fine since git itself never writes this \
         shape, but pinned so a future tighter validation is a deliberate \
         choice not an accident"
    );
}
