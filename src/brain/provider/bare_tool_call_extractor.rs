//! Bare `{"name": "<tool>", "arguments": {...}}` tool-call extractor.
//!
//! Some providers — notably qwen-3.7-max-thinking via dialagram on
//! 2026-06-02 — emit the same tool call TWICE in one assistant
//! response: once as a proper structured `tool_calls` delta (which
//! dispatches normally), and once as a JSON-stringified copy inside
//! `delta.content`. The text copy has no `<tool_call>` wrapper, no
//! `tool_call:` marker, no envelope — just the bare object dropped
//! into prose. Without a dedicated pass it survives to whatever
//! channel renders the assistant text (Telegram, Discord, etc.) and
//! looks like the user pasted the raw tool args into the chat.
//!
//! This module lives outside the main `custom_openai_compatible.rs`
//! extractor because that file is already 5000+ lines of inline
//! passes and shouldn't carry another 100-line pattern. The public
//! entry point is `extract_bare_name_args_calls`; the host extractor
//! folds the returned matches into its own `tool_calls` and
//! `strip_ranges` accumulators.

use super::custom_openai_compatible::{KNOWN_TOOL_NAMES, extract_balanced_json};
use serde_json::Value;

/// One bare `{"name":..., "arguments":...}` object found in the
/// content text. `strip_start..strip_end` is the byte range to
/// remove from the visible text. `already_in_existing` is set when
/// the host extractor has already parsed the same (name, args) pair
/// via a structured tool_calls delta — in that case the caller
/// strips but does NOT add a duplicate call.
pub(crate) struct BareToolCallMatch {
    pub name: String,
    pub args: Value,
    pub strip_start: usize,
    pub strip_end: usize,
    pub already_in_existing: bool,
}

/// Cheap precondition: a bare-tool-call extraction is only possible
/// when the text contains BOTH a `"name"` key and an `"arguments"`
/// key. The host calls this before the full scan to short-circuit
/// the common case where no bare tool-call shape is present.
pub(crate) fn has_bare_name_args_signal(text: &str) -> bool {
    (text.contains("\"name\":") || text.contains("\"name\" :"))
        && (text.contains("\"arguments\":") || text.contains("\"arguments\" :"))
}

/// Scan `text` for bare `{"name": "<known-tool>", "arguments": {...}}`
/// objects that fall OUTSIDE `existing_strip_ranges`. Returns matches
/// in source order. The `already_in_existing` flag is set per match
/// against the host's `existing_calls` (name + args equality) so the
/// caller can dedupe against structured-path calls without re-
/// dispatching the same tool.
///
/// The pass is intentionally strict — five separate checks must all
/// hold before a match is returned:
///
///   1. `"name"` must be the FIRST key of a JSON object (preceded by
///      `{` plus optional whitespace) so prose-nested `"name"` keys
///      like `{"version": 2, "name": "x"}` don't qualify.
///   2. The balanced-brace JSON must parse via `serde_json`.
///   3. `name` must be a string.
///   4. `arguments` must be an object (or a JSON-stringified object).
///   5. The string `name` must be in `KNOWN_TOOL_NAMES`. Without this
///      gate any prose mentioning `{"name": "<arbitrary>", "arguments":
///      {...}}` — e.g. a model describing an unrelated REST API
///      schema — would dispatch as a phantom tool call.
pub(crate) fn extract_bare_name_args_calls(
    text: &str,
    existing_strip_ranges: &[(usize, usize)],
    existing_calls: &[(String, Value)],
) -> Vec<BareToolCallMatch> {
    let mut out = Vec::new();
    if !has_bare_name_args_signal(text) {
        return out;
    }

    let name_anchors = ["\"name\":", "\"name\" :"];
    let mut search_from = 0;
    loop {
        let next = name_anchors
            .iter()
            .filter_map(|a| text[search_from..].find(a).map(|p| (search_from + p, *a)))
            .min_by_key(|(p, _)| *p);
        let Some((anchor_pos, anchor_lit)) = next else {
            break;
        };
        let advance_past_anchor = || anchor_pos + anchor_lit.len();

        // Skip anchors already inside a wrapper claimed by an earlier
        // marker-based pass (so we don't strip the inner JSON of a
        // `<tool_call>{...}</tool_call>` wrapper after that wrapper has
        // already been collected).
        if in_any_range(existing_strip_ranges, anchor_pos)
            || in_any_range(&existing_ranges_from(&out), anchor_pos)
        {
            search_from = advance_past_anchor();
            continue;
        }

        // Walk back past whitespace; previous non-ws byte must be `{`
        // for this `"name"` to be the first key of a fresh object.
        let mut back = anchor_pos;
        while back > 0 {
            let b = text.as_bytes()[back - 1];
            if b.is_ascii_whitespace() || b == b'\n' || b == b'\r' {
                back -= 1;
                continue;
            }
            break;
        }
        if back == 0 || text.as_bytes()[back - 1] != b'{' {
            search_from = advance_past_anchor();
            continue;
        }
        let obj_start = back - 1;
        if in_any_range(existing_strip_ranges, obj_start)
            || in_any_range(&existing_ranges_from(&out), obj_start)
        {
            search_from = advance_past_anchor();
            continue;
        }

        let Some(consumed) = extract_balanced_json(&text[obj_start..]) else {
            search_from = advance_past_anchor();
            continue;
        };
        let obj_slice = &text[obj_start..obj_start + consumed];
        let Ok(v) = serde_json::from_str::<Value>(obj_slice) else {
            search_from = advance_past_anchor();
            continue;
        };
        let Some(name_str) = v.get("name").and_then(|n| n.as_str()) else {
            search_from = advance_past_anchor();
            continue;
        };
        if !KNOWN_TOOL_NAMES.contains(&name_str) {
            search_from = advance_past_anchor();
            continue;
        }

        let Some(args) = parse_arguments_value(v.get("arguments")) else {
            search_from = advance_past_anchor();
            continue;
        };

        let already_in_existing = existing_calls
            .iter()
            .any(|(n, a)| n == name_str && a == &args);

        let strip_end = obj_start + consumed;
        out.push(BareToolCallMatch {
            name: name_str.to_string(),
            args,
            strip_start: obj_start,
            strip_end,
            already_in_existing,
        });
        search_from = strip_end;
    }

    out
}

fn in_any_range(ranges: &[(usize, usize)], pos: usize) -> bool {
    ranges.iter().any(|(s, e)| *s <= pos && pos < *e)
}

fn existing_ranges_from(matches: &[BareToolCallMatch]) -> Vec<(usize, usize)> {
    matches
        .iter()
        .map(|m| (m.strip_start, m.strip_end))
        .collect()
}

/// `arguments` must be an object, or a JSON-string that parses to one.
/// Some providers double-encode (string of JSON instead of nested
/// object) and we want to accept both shapes — same normalisation the
/// structured `parse_tool_call_value` path applies.
fn parse_arguments_value(arguments_val: Option<&Value>) -> Option<Value> {
    match arguments_val {
        Some(v) if v.is_object() => Some(v.clone()),
        Some(v) if v.is_string() => {
            let s = v.as_str().unwrap_or("{}").trim();
            if !s.starts_with('{') || !s.ends_with('}') {
                return None;
            }
            serde_json::from_str::<Value>(s)
                .ok()
                .filter(|p| p.is_object())
        }
        _ => None,
    }
}
