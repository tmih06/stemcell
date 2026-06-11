//! Tests for the `CODE_EDIT_BLOCK` stripper.
//!
//! Regression context: 2026-05-30 14:13 the qwen-3.7-max-thinking model
//! emitted Cursor/Aider-style fenced blocks like
//!
//! ```text
//! ```dart|CODE_EDIT_BLOCK|/Users/tmih06studio/srv/dart/.../estimate_model.dart
//! enum EstimateType { ... }
//! ```
//! ```
//!
//! in the response text. Telegram's markdown→HTML converter wrapped
//! the fake language tag (`dart|CODE_EDIT_BLOCK|/abs/path`) into
//! `<code class="language-dart|CODE_EDIT_BLOCK|/...">`, which is
//! not valid Telegram HTML — the resulting parse failure dumped the
//! full file contents into the chat instead of a clean code block.
//!
//! These tests pin the strip behaviour so the regression can't
//! sneak back in.

use crate::utils::sanitize::{strip_code_edit_block_fences, strip_llm_artifacts};

#[test]
fn strips_basic_code_edit_block() {
    let input = "Here are the changes:\n\
                 \n\
                 ```dart|CODE_EDIT_BLOCK|/path/to/file.dart\n\
                 enum EstimateType { rough, refined }\n\
                 ```\n\
                 \n\
                 Done.";
    let out = strip_code_edit_block_fences(input);
    assert!(
        !out.contains("CODE_EDIT_BLOCK"),
        "marker must be stripped; got: {out}"
    );
    assert!(
        !out.contains("enum EstimateType"),
        "file body must NOT leak to channel; got: {out}"
    );
    assert!(
        out.contains("/path/to/file.dart"),
        "path should remain for user audit"
    );
    assert!(
        out.contains("edit_file"),
        "agent should be nudged toward the real tool"
    );
    assert!(out.contains("Here are the changes:"));
    assert!(out.contains("Done."));
}

#[test]
fn strips_block_with_multiple_lines_in_body() {
    let input = "Edit:\n\
                 ```rust|CODE_EDIT_BLOCK|/tmp/x.rs\n\
                 fn foo() {\n\
                     let x = 1;\n\
                     let y = 2;\n\
                     println!(\"secret\");\n\
                 }\n\
                 ```\n\
                 Trailing prose.";
    let out = strip_code_edit_block_fences(input);
    assert!(!out.contains("CODE_EDIT_BLOCK"));
    assert!(!out.contains("let x = 1"));
    assert!(!out.contains("println!(\"secret\")"));
    assert!(out.contains("/tmp/x.rs"));
    assert!(out.contains("Trailing prose."));
}

#[test]
fn strips_multiple_blocks_in_one_response() {
    // Realistic case: the model emits 3-4 blocks for a multi-file
    // refactor. Each should be replaced with its own notice.
    let input = "Step 1:\n\
                 ```dart|CODE_EDIT_BLOCK|/p/one.dart\n\
                 class One {}\n\
                 ```\n\
                 Step 2:\n\
                 ```dart|CODE_EDIT_BLOCK|/p/two.dart\n\
                 class Two {}\n\
                 ```\n\
                 Done.";
    let out = strip_code_edit_block_fences(input);
    assert!(!out.contains("class One"));
    assert!(!out.contains("class Two"));
    assert!(out.contains("/p/one.dart"));
    assert!(out.contains("/p/two.dart"));
    // Two notices, one per stripped block.
    let notices = out.matches("Agent attempted to edit").count();
    assert_eq!(notices, 2, "one notice per stripped block; got: {out}");
}

#[test]
fn handles_truncated_block_without_closer() {
    // Stream cut off mid-block (model timed out, network blip).
    // We should still drop the partial body so it doesn't leak —
    // partial file contents are no better than full ones.
    let input = "Here:\n\
                 ```dart|CODE_EDIT_BLOCK|/p/file.dart\n\
                 enum A { a,\n\
                 // ... mid-stream, no closing fence";
    let out = strip_code_edit_block_fences(input);
    assert!(!out.contains("CODE_EDIT_BLOCK"));
    assert!(!out.contains("enum A"));
    assert!(out.contains("/p/file.dart"));
}

#[test]
fn leaves_normal_code_blocks_untouched() {
    let input = "Here's an example:\n\
                 ```dart\n\
                 void main() {}\n\
                 ```\n\
                 The agent emitted this as documentation, not as an edit.";
    let out = strip_code_edit_block_fences(input);
    assert!(
        out.contains("void main()"),
        "normal code blocks must survive"
    );
    assert!(out.contains("```dart"), "fence intact");
}

#[test]
fn leaves_prose_mentioning_code_edit_block_untouched() {
    // Defensive: if a user or doc legitimately discusses the marker
    // name (e.g. in a bug report), we should NOT strip the
    // surrounding text — only fenced blocks that actually OPEN with
    // it.
    let input = "We saw a CODE_EDIT_BLOCK regression yesterday. \
                 The agent emitted it instead of calling edit_file.";
    let out = strip_code_edit_block_fences(input);
    assert_eq!(out, input, "prose mention must not be stripped");
}

#[test]
fn handles_leading_whitespace_on_fence() {
    // Some models indent the fence by 1-2 spaces inside lists.
    let input = "Steps:\n  ```dart|CODE_EDIT_BLOCK|/x.dart\n  content\n  ```\nDone.";
    let out = strip_code_edit_block_fences(input);
    assert!(!out.contains("CODE_EDIT_BLOCK"));
    assert!(out.contains("/x.dart"));
}

#[test]
fn extracts_path_from_complex_real_world_path() {
    // The actual incident: a deeply nested macOS path.
    let path = "/Users/tmih06studio/srv/dart/estimerstravaux/lib/models/estimate_model.dart";
    let input = format!(
        "Extending the data model.\n\n\
         ```dart|CODE_EDIT_BLOCK|{path}\n\
         enum EstimateType {{ rough, refined, finalEstimate }}\n\
         ```\n\
         Done."
    );
    let out = strip_code_edit_block_fences(&input);
    assert!(out.contains(path), "path must round-trip; got: {out}");
    assert!(!out.contains("enum EstimateType"));
}

#[test]
fn strip_llm_artifacts_runs_the_code_edit_block_pass() {
    // The integration point: strip_llm_artifacts is what channel
    // handlers actually call, so it must dispatch to the new pass.
    let input = "Result:\n\
                 ```dart|CODE_EDIT_BLOCK|/x.dart\n\
                 enum X {}\n\
                 ```";
    let out = strip_llm_artifacts(input);
    assert!(!out.contains("CODE_EDIT_BLOCK"));
    assert!(!out.contains("enum X"));
}

#[test]
fn block_with_no_path_still_strips_safely() {
    let input = "```dart|CODE_EDIT_BLOCK|\nbody\n```";
    let out = strip_code_edit_block_fences(input);
    assert!(!out.contains("CODE_EDIT_BLOCK"));
    assert!(!out.contains("body"));
    assert!(out.contains("(unknown)"), "fallback path label; got: {out}");
}
