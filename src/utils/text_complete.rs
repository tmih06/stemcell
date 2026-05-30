//! Heuristic check for whether an LLM response text "looks complete".
//!
//! Used to distinguish two situations that look identical on the
//! wire when a stream ends without `[DONE]` / `MessageStop`:
//!
//! 1. **Connection truly dropped mid-response** — the text ends in
//!    the middle of a sentence, mid-word, with an unclosed code
//!    fence, etc. The right reaction is to retry the request.
//!
//! 2. **Provider doesn't honour `[DONE]`** — the stream delivered a
//!    full coherent response, then the TCP connection closed
//!    without an explicit termination marker. Observed on
//!    `dialagram` + `qwen-3.7-max-thinking` (2026-05-30). Retrying
//!    here regenerates the same content and pegs the
//!    "is responding..." indicator for minutes.
//!
//! Returning `true` from `text_looks_complete` lets the caller
//! synthesise a `StopReason::EndTurn` and proceed; `false` keeps
//! the existing retry path for genuinely truncated streams.
//!
//! Conservative by design: we'd rather over-retry a slightly
//! awkward complete response than under-retry a real truncation
//! that needs another shot.

/// Minimum char count below which we won't claim "complete" no
/// matter how the text ends. Short responses are usually preambles
/// like "Let me check the README:" that genuinely ARE truncated.
const MIN_COMPLETE_CHARS: usize = 200;

/// True when the response text looks structurally complete and
/// safe to accept without a `[DONE]` marker.
///
/// Heuristics applied in order:
/// - Strip trailing whitespace and walk back.
/// - If shorter than `MIN_COMPLETE_CHARS`, return false (too short
///   to confidently claim complete).
/// - If there's an unmatched ``` code fence, return false (model
///   started a code block and the stream cut before the closer).
/// - If the last non-whitespace character is a sentence terminator
///   (`.`, `!`, `?`), a closing bracket / quote, a list-item
///   period, an emoji, or a closing fence, return true.
/// - If the last non-whitespace character looks like a clear
///   mid-sentence marker (`:`, `,`, `;`, `-`, opening bracket /
///   quote), return false.
/// - Default: true. We've passed the size + fence checks; the
///   remaining cases (ends in a word, a number, etc.) are rare on
///   real completions but ALSO rare on truncations, and the cost
///   of a spurious retry is much higher than the cost of accepting
///   a slightly informal ending.
pub fn text_looks_complete(text: &str) -> bool {
    let trimmed = text.trim_end();
    if trimmed.chars().count() < MIN_COMPLETE_CHARS {
        return false;
    }
    if has_unmatched_code_fence(trimmed) {
        return false;
    }
    let Some(last) = trimmed.chars().next_back() else {
        return false;
    };
    // Sentence terminators / closers — strong signals the text
    // ended where the model intended.
    if matches!(
        last,
        '.' | '!' | '?' | '"' | '\'' | ')' | ']' | '}' | '`' | '*' | '_' | '\u{2026}' // …
    ) {
        return true;
    }
    // Clear mid-sentence markers — the model was still going.
    if matches!(last, ':' | ',' | ';' | '-' | '(' | '[' | '{') {
        return false;
    }
    // Letters / digits / other punctuation: assume complete. The
    // bias is toward "don't retry" because spurious retries are
    // user-visible (minutes of "is responding...") while occasional
    // accept-truncated is at worst one awkward message.
    true
}

/// Count of ` ``` ` (triple-backtick) fences and report whether
/// it's odd (= one unclosed). A naive substring count is fine
/// because fences are line-anchored in practice and the model
/// rarely emits triple backticks inside inline code.
fn has_unmatched_code_fence(text: &str) -> bool {
    let fences = text.matches("```").count();
    fences.is_multiple_of(2).not()
}

// std::ops::Not isn't in scope by default for bool; bring it in
// without polluting the module top.
use std::ops::Not;
