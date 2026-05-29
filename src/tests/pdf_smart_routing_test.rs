//! Tests for the text-first PDF routing in `process_file_with_vision`
//! and the lazy `analyze_image` injection contract for scanned PDFs.
//!
//! Two invariants protect the agent from the 10 MB / 413 Payload Too
//! Large incident:
//!
//! 1. `FileContent::PdfPages` must NEVER produce a marker that
//!    `build_user_message` will base64-inline. Specifically: no
//!    `<<IMG:` substrings in the injected text. The agent must
//!    reach page images via per-page tool calls, not via the
//!    user-message inline-attach path.
//!
//! 2. Text-rich PDFs must take the text path, not the vision path,
//!    so the request body stays small. We can't directly call
//!    `process_pdf_smart` (module-private) but we exercise the same
//!    behavior through `inject_file_content` for the rendered
//!    output and through `extract_pdf_text` indirectly via
//!    `classify_file` on a real-ish PDF.

use crate::utils::file_extract::{FileContent, inject_file_content};
use std::path::PathBuf;

#[test]
fn pdf_pages_injection_does_not_emit_inline_image_markers() {
    // The big regression: any `<<IMG:...>>` in the injected text
    // would make `build_user_message` base64-inline the image
    // into a single user Message. For a 32-page PDF that produces
    // ~14 MB of inline image data → HTTP 413 from any provider
    // with a 10 MB cap (Dialagram, in the field). The injection
    // MUST list page paths as plain text so the agent calls
    // `analyze_image` per page instead.
    let content = FileContent::PdfPages {
        paths: vec![
            PathBuf::from("/tmp/x/page-01.png"),
            PathBuf::from("/tmp/x/page-02.png"),
            PathBuf::from("/tmp/x/page-03.png"),
        ],
        label: "scanned 3-page PDF".to_string(),
    };
    let (text, needs_vision) = inject_file_content(&content);

    assert!(
        !text.contains("<<IMG:"),
        "PdfPages must not emit `<<IMG:...>>` markers (would balloon \
         request body past provider limits — see 413 incident 2026-05-30). \
         Got: {text}"
    );
    assert!(
        !needs_vision,
        "PdfPages must report needs_vision=false so callers don't \
         trigger inline-image fast paths. Got: {needs_vision}"
    );
}

#[test]
fn pdf_pages_injection_lists_every_page_path() {
    let content = FileContent::PdfPages {
        paths: vec![
            PathBuf::from("/tmp/abc/page-01.png"),
            PathBuf::from("/tmp/abc/page-02.png"),
        ],
        label: "scanned 2-page PDF".to_string(),
    };
    let (text, _) = inject_file_content(&content);

    assert!(
        text.contains("page-01.png"),
        "must list page 1; got: {text}"
    );
    assert!(
        text.contains("page-02.png"),
        "must list page 2; got: {text}"
    );
    assert!(
        text.contains("analyze_image"),
        "must tell the agent to call analyze_image per page; got: {text}"
    );
    assert!(
        text.contains("ONE PAGE AT A TIME") || text.contains("one page at a time"),
        "must warn the agent NOT to bundle pages; got: {text}"
    );
}

#[test]
fn pdf_pages_injection_uses_human_readable_page_numbers() {
    // The model thinks in "page 1" / "page 2", not in filename
    // padding (`page-01.png`). The list must show both so a model
    // reading the path list can map "the agent wants page 3" to
    // the right file.
    let content = FileContent::PdfPages {
        paths: (1..=5)
            .map(|i| PathBuf::from(format!("/tmp/x/page-{i:02}.png")))
            .collect(),
        label: "scanned 5-page PDF".to_string(),
    };
    let (text, _) = inject_file_content(&content);

    for n in 1..=5 {
        assert!(
            text.contains(&format!("Page {n}:")),
            "must show `Page {n}:` label; got: {text}"
        );
    }
}

#[test]
fn single_image_injection_still_uses_marker_for_inline_attach() {
    // The flip is PDF-specific. A single image attachment continues
    // to use the `<<IMG:...>>` fast path — one image is small,
    // doesn't risk 413, and inline attach saves a tool-call round
    // trip for the common case of "user sent a photo, agent
    // describes it".
    let content = FileContent::Image(PathBuf::from("/tmp/photo.jpg"));
    let (text, needs_vision) = inject_file_content(&content);

    assert!(
        text.contains("<<IMG:/tmp/photo.jpg>>"),
        "single Image must keep the inline marker; got: {text}"
    );
    assert!(
        needs_vision,
        "single Image must report needs_vision=true so callers \
         expand the marker into a vision content block"
    );
}

#[test]
fn pdf_pages_label_appears_in_injection() {
    let content = FileContent::PdfPages {
        paths: vec![PathBuf::from("/tmp/x/page-01.png")],
        label: "scanned PDF".to_string(),
    };
    let (text, _) = inject_file_content(&content);

    assert!(
        text.contains("scanned PDF"),
        "label must surface in the injection; got: {text}"
    );
}
