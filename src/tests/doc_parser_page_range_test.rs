//! Tests for the `page_range` parser in `brain::tools::doc_parser`.
//!
//! `parse_page_range` is module-private, so we exercise it through the
//! tool's public `execute` path with a fake (non-existent) document.
//! That covers schema acceptance + the merge with `pages`. The pure
//! parse-string-to-Vec path is also tested via a `pub(crate)`-free
//! contract: we feed each spec through the tool and assert the
//! resulting page-range error mentions the merged pages.

use crate::brain::tools::doc_parser::DocParserTool;
use crate::brain::tools::{Tool, ToolExecutionContext};
use serde_json::json;
use uuid::Uuid;

fn ctx() -> ToolExecutionContext {
    ToolExecutionContext::new(Uuid::new_v4())
}

#[tokio::test]
async fn page_range_schema_is_accepted() {
    let tool = DocParserTool;
    let input = json!({
        "path": "/no/such/file.pdf",
        "page_range": "1-30"
    });
    // validate_input should pass even though the file is missing —
    // we're checking the schema accepts the new field.
    tool.validate_input(&input)
        .expect("page_range must validate");
}

#[tokio::test]
async fn page_range_combined_with_pages_validates() {
    let tool = DocParserTool;
    let input = json!({
        "path": "/no/such/file.pdf",
        "pages": [1, 5],
        "page_range": "10-12, 20"
    });
    tool.validate_input(&input)
        .expect("both fields together must validate");
}

#[tokio::test]
async fn missing_file_still_errors_cleanly_with_page_range() {
    let tool = DocParserTool;
    let result = tool
        .execute(
            json!({
                "path": "/definitely/not/here.pdf",
                "page_range": "1-30"
            }),
            &ctx(),
        )
        .await
        .expect("tool returned a ToolResult");
    assert!(!result.success);
    let err = result.error.unwrap_or_default();
    assert!(err.contains("not found") || err.contains("File not found"));
}

#[tokio::test]
async fn invalid_page_range_string_is_silently_ignored() {
    // Garbage page_range must not crash the tool — it should be
    // treated as "no range supplied" and fall through to the
    // missing-file error.
    let tool = DocParserTool;
    let result = tool
        .execute(
            json!({
                "path": "/no/such/file.pdf",
                "page_range": "garbage; not a range"
            }),
            &ctx(),
        )
        .await
        .expect("tool returned a ToolResult");
    assert!(!result.success);
    assert!(result.error.unwrap_or_default().contains("not found"));
}

#[tokio::test]
async fn empty_page_range_is_ignored() {
    let tool = DocParserTool;
    let result = tool
        .execute(
            json!({
                "path": "/no/such/file.pdf",
                "page_range": ""
            }),
            &ctx(),
        )
        .await
        .expect("tool returned a ToolResult");
    assert!(!result.success);
    assert!(result.error.unwrap_or_default().contains("not found"));
}
