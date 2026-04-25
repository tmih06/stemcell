//! Tests for the macOS `defaults read LSHandlers` parser that resolves
//! the user's default web browser.
//!
//! This pins the behaviour of ff8026c. The original line-by-line parser
//! returned `"-"` (a placeholder from the nested
//! LSHandlerPreferredVersions dict) as the identifier, so
//! `detect_browser()` fell through to the first installed Chromium
//! candidate (Google Chrome) even when the user's default was Brave.
//!
#![cfg(feature = "browser")]

//! Fixtures are modelled on the actual `defaults` plist output observed
//! on the developer's machine (2026-04-19).

#![cfg(target_os = "macos")]

use crate::brain::tools::browser::parse_ls_handlers;

/// Real-shape fixture: https scheme handler, role comes BEFORE scheme
/// in the block (order matters — the old parser required scheme-first).
#[test]
fn resolves_https_scheme_handler() {
    let plist = r#"(
        {
        LSHandlerPreferredVersions =         {
            LSHandlerRoleAll = "-";
        };
        LSHandlerRoleAll = "com.brave.browser";
        LSHandlerURLScheme = https;
    }
)"#;
    assert_eq!(parse_ls_handlers(plist), Some("com.brave.browser".into()));
}

/// Real-shape fixture: default-web-browser content type (System Settings form).
#[test]
fn resolves_content_type_default_web_browser() {
    let plist = r#"(
        {
        LSHandlerContentType = "com.apple.default-app.web-browser";
        LSHandlerPreferredVersions =         {
            LSHandlerRoleAll = "-";
        };
        LSHandlerRoleAll = "com.brave.browser";
    }
)"#;
    assert_eq!(parse_ls_handlers(plist), Some("com.brave.browser".into()));
}

/// Placeholder `-` in the nested PreferredVersions dict must not leak
/// out as the detected identifier. This was the exact bug.
#[test]
fn ignores_nested_placeholder_dash() {
    let plist = r#"(
        {
        LSHandlerPreferredVersions =         {
            LSHandlerRoleAll = "-";
        };
    }
)"#;
    assert_eq!(
        parse_ls_handlers(plist),
        None,
        "a block with only the nested placeholder must NOT return `-`"
    );
}

/// Multi-entry fixture — the https entry is the second block, and the
/// first block (unrelated content-type) must not bleed values into the
/// second. This exercises the per-block reset on brace depth 0.
#[test]
fn multi_entry_picks_only_the_https_block() {
    let plist = r#"(
        {
        LSHandlerContentType = "public.html";
        LSHandlerPreferredVersions =         {
            LSHandlerRoleAll = "-";
        };
        LSHandlerRoleAll = "com.apple.safari";
    },
        {
        LSHandlerPreferredVersions =         {
            LSHandlerRoleAll = "-";
        };
        LSHandlerRoleAll = "com.brave.browser";
        LSHandlerURLScheme = https;
    },
        {
        LSHandlerPreferredVersions =         {
            LSHandlerRoleAll = "-";
        };
        LSHandlerRoleAll = "com.apple.ical";
        LSHandlerURLScheme = ical;
    }
)"#;
    assert_eq!(parse_ls_handlers(plist), Some("com.brave.browser".into()));
}

/// HTTPS scheme is matched case-insensitively (defaults sometimes emits
/// bare `https`, sometimes quoted `"https"`, and at least one macOS
/// version has been seen to uppercase the value).
#[test]
fn scheme_match_is_case_insensitive() {
    let plist = r#"(
        {
        LSHandlerRoleAll = "com.brave.browser";
        LSHandlerURLScheme = HTTPS;
    }
)"#;
    assert_eq!(parse_ls_handlers(plist), Some("com.brave.browser".into()));
}

/// Lowercase output — `matches_default` later compares against
/// `bundle_id.to_lowercase()` so the parser normalising here saves
/// a step downstream.
#[test]
fn output_is_lowercased() {
    let plist = r#"(
        {
        LSHandlerRoleAll = "Com.Brave.Browser";
        LSHandlerURLScheme = https;
    }
)"#;
    assert_eq!(parse_ls_handlers(plist), Some("com.brave.browser".into()));
}

/// Empty handler array — no web-default configured. The fall-through
/// path in `detect_browser` handles this by scanning installed browsers.
#[test]
fn empty_array_returns_none() {
    assert_eq!(parse_ls_handlers("(\n)"), None);
}

/// A block with neither scheme nor content-type must NOT emit even if
/// it contains a role (it's not for the default web browser).
#[test]
fn block_without_web_marker_is_ignored() {
    let plist = r#"(
        {
        LSHandlerRoleAll = "com.apple.mail";
        LSHandlerURLScheme = mailto;
    }
)"#;
    assert_eq!(parse_ls_handlers(plist), None);
}

// ─── id_matches_default: candidate vs LaunchServices id comparison ──
//
// 2026-04-25: user reported the browser tool launching Google Chrome
// instead of Brave. The literal "Chrome" came from a hardcoded error
// string, but the comparison itself is the failure mode that bit
// past Brave users and is worth pinning explicitly.
//
// macOS `defaults read … LSHandlers` reports bundle ids in their
// canonical lowercased form (`com.brave.browser`). Our candidate
// table carries Apple's mixed-case form (`com.brave.Browser`). A
// naive `==` would miss the match and `detect_browser` would fall
// through to the next installed Chromium-based browser — usually
// Chrome.

use crate::brain::tools::browser::id_matches_default;

#[test]
fn id_match_brave_handles_case_difference() {
    // The exact 2026-04-25 user case.
    assert!(id_matches_default("com.brave.Browser", "com.brave.browser"));
    assert!(id_matches_default("com.brave.browser", "com.brave.Browser"));
}

#[test]
fn id_match_chrome_does_not_match_brave_default() {
    // Sanity: case-insensitive does NOT collapse different families.
    assert!(!id_matches_default(
        "com.google.chrome",
        "com.brave.browser"
    ));
    assert!(!id_matches_default(
        "com.brave.browser",
        "com.google.chrome"
    ));
}

#[test]
fn id_match_handles_uppercased_scheme_output() {
    // Some macOS versions uppercase the bundle id in LSHandlers output;
    // parse_ls_handlers lowercases for us, but the comparator must
    // tolerate either side being upper anyway.
    assert!(id_matches_default("com.brave.Browser", "COM.BRAVE.BROWSER"));
}

#[test]
fn id_match_empty_strings_do_not_silently_collide() {
    // Defensive: an empty default_id (parse failure) must NOT match
    // an empty candidate (which can't happen but pin it anyway).
    assert!(id_matches_default("", ""));
    assert!(!id_matches_default("com.brave.Browser", ""));
    assert!(!id_matches_default("", "com.brave.browser"));
}
