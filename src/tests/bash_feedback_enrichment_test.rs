//! Tests for `enrich_metadata` — the failure-snippet enricher that
//! appends the bash command text to the feedback ledger row so RSI
//! can categorize failures by subsystem (git vs python vs docker
//! etc.) instead of treating every `bash` failure as one blob
//! (issue #132).
//!
//! Without enrichment a query like "how often does git fail?" needs
//! to walk every per-message tool execution. After the fix the
//! `meta` column carries `cmd=<command>`, so SQL like
//! `WHERE dimension = 'bash' AND meta LIKE '%cmd=git%'` works.

use crate::brain::agent::service::feedback::enrich_metadata;
use serde_json::json;

#[test]
fn bash_failure_appends_cmd_to_snippet() {
    let input = json!({ "command": "git rebase main" });
    let result = enrich_metadata(
        "bash",
        false,
        Some("Command exited with code 1"),
        Some(&input),
    );
    assert_eq!(
        result,
        Some("Command exited with code 1 | cmd=git rebase main".to_string())
    );
}

#[test]
fn bash_success_does_not_append_cmd() {
    // Success rows don't need the command for subsystem analysis —
    // the ledger only enriches the failure path so RSI can focus
    // its categorization budget on failures.
    let input = json!({ "command": "git status" });
    let result = enrich_metadata("bash", true, None, Some(&input));
    assert_eq!(result, None);
}

#[test]
fn bash_failure_with_no_snippet_still_emits_cmd() {
    // Some failure paths land with no error string yet (e.g. user
    // denied approval before stderr existed). The command alone is
    // still useful — RSI sees "user denied a git command" vs
    // "user denied a docker command".
    let input = json!({ "command": "docker build ." });
    let result = enrich_metadata("bash", false, None, Some(&input));
    assert_eq!(result, Some("cmd=docker build .".to_string()));
}

#[test]
fn non_bash_failure_passes_snippet_through_unchanged() {
    // The enrichment is bash-only for now. A failure on
    // `parse_document` keeps its snippet verbatim.
    let input = json!({ "path": "/tmp/x.pdf" });
    let result = enrich_metadata(
        "parse_document",
        false,
        Some("File not found"),
        Some(&input),
    );
    assert_eq!(result, Some("File not found".to_string()));
}

#[test]
fn bash_failure_without_command_field_falls_back_to_snippet() {
    // Defensive: a malformed bash input shouldn't break the
    // recorder. We get only the original snippet.
    let input = json!({ "something_else": "..." });
    let result = enrich_metadata("bash", false, Some("Some error"), Some(&input));
    assert_eq!(result, Some("Some error".to_string()));
}

#[test]
fn bash_failure_with_none_input_falls_back_to_snippet() {
    // The user-denied path before execution doesn't have a
    // meaningful input; the recorder should still produce a
    // ledger entry.
    let result = enrich_metadata("bash", false, Some("user_denied_approval"), None);
    assert_eq!(result, Some("user_denied_approval".to_string()));
}

#[test]
fn empty_command_string_is_not_appended() {
    // Edge: a literal empty command. Don't emit `cmd=` because the
    // subsystem prefix LIKE queries would still match.
    let input = json!({ "command": "" });
    let result = enrich_metadata("bash", false, Some("error"), Some(&input));
    assert_eq!(result, Some("error".to_string()));
}

#[test]
fn very_long_command_is_truncated_to_300_chars() {
    let long_cmd = "git push origin main && ".repeat(200); // ~4800 chars
    let input = json!({ "command": long_cmd });
    let result = enrich_metadata("bash", false, Some("error"), Some(&input)).unwrap();
    // Snippet (~5 chars) + " | cmd=" (7) + truncated command (300) = 312 chars.
    assert!(
        result.len() <= 312,
        "command should be capped at 300 chars; got {} char meta: {}",
        result.len(),
        &result[..result.len().min(120)]
    );
    assert!(result.starts_with("error | cmd=git push"));
}

#[test]
fn snippet_with_special_chars_is_preserved() {
    // Real bash errors contain newlines, quotes, etc. The enricher
    // should not mangle them — the | cmd= delimiter is appended as
    // a marker, not as a normalizer.
    let input = json!({ "command": "ls /nonexistent" });
    let snippet = "ls: cannot access '/nonexistent': No such file or directory\nexit code: 2";
    let result = enrich_metadata("bash", false, Some(snippet), Some(&input)).unwrap();
    assert!(result.contains("No such file or directory"));
    assert!(result.contains("cmd=ls /nonexistent"));
    assert!(result.contains('\n'));
}

#[test]
fn realistic_git_failure() {
    let input = json!({ "command": "git rebase --continue" });
    let snippet = "error: could not apply abc1234... fix typo\nhint: Resolve conflicts then run git rebase --continue";
    let result = enrich_metadata("bash", false, Some(snippet), Some(&input)).unwrap();
    assert!(result.starts_with("error: could not apply"));
    assert!(result.ends_with("cmd=git rebase --continue"));
}

#[test]
fn realistic_python_module_not_found() {
    let input = json!({ "command": "python3 -c \"import openpyxl\"" });
    let snippet = "ModuleNotFoundError: No module named 'openpyxl'";
    let result = enrich_metadata("bash", false, Some(snippet), Some(&input)).unwrap();
    assert!(result.contains("ModuleNotFoundError"));
    assert!(result.contains("cmd=python3 -c"));
}

#[test]
fn realistic_timeout() {
    let input = json!({ "command": "cargo build --release", "timeout_secs": 60 });
    let snippet = "Command timed out after 120 seconds";
    let result = enrich_metadata("bash", false, Some(snippet), Some(&input)).unwrap();
    assert!(result.contains("timed out"));
    assert!(result.contains("cmd=cargo build --release"));
}

#[test]
fn command_with_unicode_is_preserved() {
    // Real-world bash often has paths with accents — Mac users
    // especially. The truncation should respect char boundaries.
    let input = json!({ "command": "ls /Users/José/Documents" });
    let result = enrich_metadata("bash", false, Some("not found"), Some(&input)).unwrap();
    assert!(result.contains("José"));
}

#[test]
fn non_bash_tool_input_is_ignored_even_if_it_has_command_field() {
    // Some hypothetical other tool could have its own `command`
    // field. We only enrich bash so unrelated tools' metadata
    // stays clean.
    let input = json!({ "command": "some_arg" });
    let result = enrich_metadata("custom_tool", false, Some("snip"), Some(&input));
    assert_eq!(result, Some("snip".to_string()));
}
