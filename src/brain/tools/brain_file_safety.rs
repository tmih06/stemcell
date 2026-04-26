//! Brain-file write safety: backup + append-only enforcement.
//!
//! The 2026-04-26 RSI agent rewrote `~/.opencrabs/TOOLS.md` from 33 KB
//! down to a stub by passing the entire file as `old_content` to
//! `self_improve` action="update". Same pattern hit `MEMORY.md`. Brain
//! files are append-only by user policy: removal is allowed only when
//! it deduplicates content that already exists elsewhere in the file.
//!
//! This module is the chokepoint every brain-file mutation must go
//! through. It:
//!
//! - Snapshots the file to `<name>.YYYY-MM-DDTHHMMSS.bak` before any write
//! - Rejects writes that shrink a protected file unless the caller
//!   explicitly opts into a dedup intent and the removal really is a
//!   duplicate that survives elsewhere in the result.

use std::path::Path;

/// Brain files that follow the append-only contract. Any mutation that
/// shrinks one of these without a justified dedup must be rejected.
const PROTECTED_BRAIN_FILES: &[&str] = &[
    "SOUL.md",
    "USER.md",
    "AGENTS.md",
    "TOOLS.md",
    "CODE.md",
    "SECURITY.md",
    "MEMORY.md",
    "BOOT.md",
    "IDENTITY.md",
];

/// True if `name` (a bare file name, no directories) is one of the
/// brain files the append-only contract applies to.
pub fn is_protected_brain_file(name: &str) -> bool {
    PROTECTED_BRAIN_FILES
        .iter()
        .any(|p| p.eq_ignore_ascii_case(name))
}

/// True if `path`'s file name is a protected brain file.
pub fn is_protected_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(is_protected_brain_file)
        .unwrap_or(false)
}

/// Snapshot `path` to `<path>.YYYY-MM-DDTHHMMSS.bak` before a mutation
/// happens. No-op when `path` doesn't exist yet (nothing to back up).
/// Returns the backup path on success.
pub fn backup_before_write(path: &Path) -> std::io::Result<Option<std::path::PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let stamp = chrono::Utc::now().format("%Y-%m-%dT%H%M%S");
    let mut backup = path.as_os_str().to_owned();
    backup.push(format!(".{stamp}.bak"));
    let backup = std::path::PathBuf::from(backup);
    std::fs::copy(path, &backup)?;
    Ok(Some(backup))
}

/// Decision returned by [`check_no_shrink`].
#[derive(Debug, PartialEq, Eq)]
pub enum ShrinkCheck {
    /// Safe to write — the new content does not shrink a protected file.
    Allowed,
    /// Reject — would remove `removed_bytes` from a protected brain file.
    /// `message` is a user-facing explanation suitable for ToolResult::error.
    Rejected { message: String },
}

/// Enforce append-only on protected brain files. `path` is the file
/// being mutated, `existing` is its current content (empty if new),
/// `updated` is what the caller wants to write.
///
/// Allows shrinking when the caller explicitly opts in via
/// `dedup_intent=true` AND every byte that disappeared can still be
/// found in the result (i.e. it really was a duplicate). Otherwise
/// any byte loss on a protected file is a hard reject.
pub fn check_no_shrink(
    path: &Path,
    existing: &str,
    updated: &str,
    dedup_intent: bool,
) -> ShrinkCheck {
    if !is_protected_path(path) {
        return ShrinkCheck::Allowed;
    }
    if updated.len() >= existing.len() {
        return ShrinkCheck::Allowed;
    }
    let removed_bytes = existing.len().saturating_sub(updated.len());
    let label = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("brain file");

    if dedup_intent && shrink_only_drops_duplicates(existing, updated) {
        return ShrinkCheck::Allowed;
    }

    let hint = if dedup_intent {
        " (dedup_intent was set, but the bytes removed do not all reappear in the result \
          — that's not deduplication, that's deletion)"
    } else {
        ""
    };
    ShrinkCheck::Rejected {
        message: format!(
            "Refusing to shrink protected brain file {label} by {removed_bytes} bytes. \
             Brain files are append-only — use action='apply' / operation='append' to \
             add new content. Removals are only allowed for genuine deduplication, and \
             must opt in via dedup_intent=true with a result that still contains every \
             unique line of the original.{hint}"
        ),
    }
}

/// Verifies the shrink really is a dedup: every line that was in
/// `existing` must still be present in `updated` (it's allowed to
/// appear once instead of multiple times). If any line disappears
/// completely, this isn't dedup — it's deletion.
fn shrink_only_drops_duplicates(existing: &str, updated: &str) -> bool {
    let updated_lines: std::collections::HashSet<&str> =
        updated.lines().map(str::trim_end).collect();
    for line in existing.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            continue;
        }
        if !updated_lines.contains(trimmed) {
            return false;
        }
    }
    true
}
