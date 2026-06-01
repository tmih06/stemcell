//! Tests for `tui::render::tools::unescape_display_string`.
//!
//! Regression: a qwen Edit tool call emitted `old_string` /
//! `new_string` payloads with literal `\n` (backslash + n) instead
//! of actual newline bytes. The renderer's `s.lines()` split only
//! catches real `\n` bytes, so the whole multi-line payload became
//! one logical line that ratatui then wrapped at the screen width
//! and displayed as a run-together soup like
//! `chars. Cap is 40. Shorten it.",\n  opt,\n  opt.len()\n  )));\n`
//! with the escape sequences visible.
//!
//! Fix: `unescape_display_string` translates the common JSON-shape
//! escapes (`\n`, `\t`, `\r`, `\\`, `\"`) back to their textual form
//! before the line split, so the renderer sees the multi-line
//! structure the model intended.

use crate::tui::render::unescape_display_string;

#[test]
fn newline_escape_becomes_real_newline() {
    let input = r"line one\nline two\nline three";
    let out = unescape_display_string(input);
    assert_eq!(out, "line one\nline two\nline three");
    // and lines() splits on the real newline now
    assert_eq!(out.lines().count(), 3);
}

#[test]
fn tab_escape_becomes_four_spaces() {
    // We expand `\t` to four spaces (not a real tab) so the TUI
    // display matches a model's intended indentation. Real tabs
    // render unpredictably in ratatui depending on terminal config.
    let out = unescape_display_string(r"a\tb");
    assert_eq!(out, "a    b");
}

#[test]
fn carriage_return_is_dropped() {
    // Drop literal `\r` — the renderer splits on `\n` only and a
    // trailing `\r` would otherwise show as a stray control char on
    // unix terminals.
    let out = unescape_display_string("foo\\r\\nbar");
    assert_eq!(out, "foo\nbar");
}

#[test]
fn quote_escape_becomes_real_quote() {
    let out = unescape_display_string(r#"a\"b\"c"#);
    assert_eq!(out, r#"a"b"c"#);
}

#[test]
fn double_backslash_becomes_single() {
    let out = unescape_display_string(r"path\\to\\file");
    assert_eq!(out, r"path\to\file");
}

#[test]
fn no_escapes_returns_input_unchanged() {
    // Fast-path: strings without backslashes skip the work.
    let plain = "fn foo() {\n    println!(\"hi\");\n}";
    let out = unescape_display_string(plain);
    assert_eq!(out, plain);
}

#[test]
fn unknown_escape_passes_through_verbatim() {
    // `\x` isn't one of the sequences we handle. The backslash
    // stays so the user can still see what the model produced
    // (better than silently dropping characters).
    let out = unescape_display_string(r"hex\x20space");
    assert_eq!(out, r"hex\x20space");
}

#[test]
fn reproduces_user_screenshot_leak_shape() {
    // The exact failure mode reported 2026-06-01: a Rust function
    // body with literal `\n` separators got rendered as a single
    // wrapped line. After the unescape it splits cleanly across
    // multiple display lines.
    let input = r#"        "Option '{}' is {} chars. Cap is 40. Shorten it.",\n        opt,\n        opt.len()\n    )));\n    }\n    if !seen.insert(opt.as_str())"#;
    let out = unescape_display_string(input);
    let count = out.lines().count();
    assert!(
        count >= 5,
        "input should split to several display lines after unescape; got {count}: {out:?}"
    );
    // No literal `\n` remains
    assert!(
        !out.contains("\\n"),
        "no literal backslash-n must survive: {out:?}"
    );
}

#[test]
fn trailing_backslash_does_not_panic() {
    // `\` at end of string — there's no follow char to consume.
    // Must not panic; just pass through.
    let out = unescape_display_string(r"foo\");
    assert_eq!(out, r"foo\");
}

#[test]
fn empty_input_returns_empty() {
    assert_eq!(unescape_display_string(""), "");
}
