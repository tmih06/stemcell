//! Tests for `posix_single_quote` — the POSIX single-quote escaper used to
//! safely embed the SSH askpass password-file path into a /bin/sh script.
//!
//! Single-quoted strings in /bin/sh are 100% literal except for the closing
//! `'`, so the escape rule is: replace every `'` in the input with `'\''`
//! (close, escaped quote, reopen) and wrap the result in single quotes.

use crate::brain::tools::bash::posix_single_quote;

#[test]
fn plain_path_is_wrapped_in_single_quotes() {
    assert_eq!(posix_single_quote("/tmp/file"), "'/tmp/file'");
}

#[test]
fn empty_string_becomes_empty_quoted_string() {
    assert_eq!(posix_single_quote(""), "''");
}

#[test]
fn spaces_are_preserved_inside_quotes() {
    assert_eq!(
        posix_single_quote("/tmp/with space/x"),
        "'/tmp/with space/x'"
    );
}

#[test]
fn dollar_signs_are_inert_inside_single_quotes() {
    // /bin/sh does not expand $VAR inside single quotes — no escaping needed.
    assert_eq!(posix_single_quote("/tmp/$HOME/x"), "'/tmp/$HOME/x'");
}

#[test]
fn backticks_are_inert_inside_single_quotes() {
    assert_eq!(posix_single_quote("/tmp/`id`/x"), "'/tmp/`id`/x'");
}

#[test]
fn double_quotes_are_inert_inside_single_quotes() {
    assert_eq!(posix_single_quote("/tmp/\"x\"/y"), "'/tmp/\"x\"/y'");
}

#[test]
fn embedded_single_quote_is_escaped() {
    // The one character that *can* break out: a literal `'`. It must close
    // the open quote, emit a backslash-escaped quote, and reopen.
    assert_eq!(posix_single_quote("a'b"), "'a'\\''b'");
}

#[test]
fn multiple_single_quotes_are_each_escaped() {
    assert_eq!(posix_single_quote("'x'"), "''\\''x'\\'''");
}

#[test]
fn newline_inside_quotes_is_preserved_verbatim() {
    // /bin/sh keeps newlines literal inside single quotes; the script will
    // still parse correctly because `cat` only sees one argument.
    assert_eq!(posix_single_quote("a\nb"), "'a\nb'");
}
