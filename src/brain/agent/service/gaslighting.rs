//! Gaslighting-preamble detection.
//!
//! Catches assistant text that lies about tool/capability availability —
//! typically a refusal opener emitted alongside a real `tool_use` block,
//! produced by certain provider quirks (notably dialagram qwen-thinking).
//! See parent `helpers.rs` history for the catalogue of incidents that
//! seeded these phrases.

/// Refusal/gaslighting phrases harvested from real dialagram qwen-thinking
/// streams (see logs 2026-04-08). Every phrase was observed in an assistant
/// turn that ALSO contained a valid `tool_use` block that executed
/// successfully — i.e. the model claimed tools were broken while
/// simultaneously calling them. These substrings are matched
/// case-insensitively.
const GASLIGHTING_REFUSAL_PHRASES: &[&str] = &[
    // "tools are broken" family
    "tools aren't responding",
    "tools are not responding",
    "tools are flaky",
    "tools are still flaky",
    "tools appear to be",
    "tools appear broken",
    "tools appear unavailable",
    "appear to be unavailable",
    "tools are unavailable",
    "tools are currently unavailable",
    "tools are disabled",
    "tools are not loading",
    "tools are not available",
    // "not currently available" family (23:58 incident — vision tool variant)
    "isn't currently available",
    "is not currently available",
    "not currently available",
    "tool isn't currently",
    "tool is not currently",
    "vision tool isn't",
    "vision tool is not",
    "vision integration",
    "despite being in my tool list",
    "despite being in the tool list",
    "despite appearing in",
    "even though it appears in",
    // "not registered" family
    "isn't actually registered",
    "is not actually registered",
    "not actually registered",
    "isn't registered",
    "isn't loaded",
    "is not loaded",
    "isn't in the registry",
    "not in the registry",
    // "runtime mismatch" family
    "mismatch between the advertised",
    "advertised capabilities",
    "runtime hiccup",
    "might be a runtime",
    "might be a configuration issue",
    "configuration issue",
    "runtime issue",
    "runtime glitch",
    "underlying system disruption",
    "system disruption",
    "provider glitch",
    "provider hiccup",
    // "can't execute / unable to" family
    "can't execute the tool",
    "cannot execute the tool",
    "unable to execute the tool",
    "unable to invoke",
    "unable to call the tool",
    "unable to retrieve",
    "unable to analyze",
    "unable to analyse",
    "unable to process",
    "unable to view",
    "unable to see",
    "unable to read",
    "tool execution failed before it started",
    // "user workaround" family — when the model asks the user to manually
    // re-upload or describe content that the tool WOULD handle. These are
    // pure gaslighting preambles emitted alongside the real tool_use call.
    "try uploading it again",
    "try uploading the image again",
    "upload it again",
    "upload the image again",
    "just tell me what's in",
    "just describe what's in",
    "or just tell me",
    "or just describe",
    "paste it as",
    "paste the image",
    "drop the path",
    "if you need image analysis",
    "for image analysis you could",
    // "no access / not in my environment" family (00:30 incident)
    "don't have access to a working",
    "do not have access to a working",
    "don't have a working",
    "tool isn't available in my",
    "tool is not available in my",
    "isn't available in my current environment",
    "not available in my current environment",
    "in my current environment",
    "working image analysis tool",
    "image analysis tool for local files",
    "upload the screenshot to a public",
    "upload the image to a public",
    "try to analyze it via url",
    "analyze it via a url",
    "analyze it via url",
    "public url (imgur",
    "(imgur, github",
    // "sandbox / tools acting up" family (2026-04-09 incident — model
    // woke up on first round convinced it was in a sandboxed container
    // and the tool layer was broken, while drafting the full answer
    // right after the preamble).
    "tools are acting up",
    "tools are acting weird",
    "tool layer is acting",
    "does not exists errors",
    "does not exist errors",
    "errors across the board",
    "getting errors across",
    "getting \"does not exist",
    "getting 'does not exist",
    "tools seem to be down",
    "tools seem broken",
    "tool system is down",
    "running in a sandbox",
    "running in a sandboxed",
    "sandboxed environment",
    "sandboxed container",
    "docker container with no",
    "no tool access in",
    // "still glitching / session weirdness" family (2026-04-09 second
    // incident — model blamed a previous model-switch for tool failures
    // while drafting the real answer in the same block).
    "tools are still glitching",
    "tools still glitching",
    "tools are glitching",
    "tool layer is glitching",
    "session state got weird",
    "session state is weird",
    "from the model switch earlier",
    "from the earlier model switch",
    "from the earlier session state",
    "turbulence from rapid model",
    "turbulence from the model",
    "some turbulence from",
    "session had some turbulence",
    "tools temporarily failing",
    "temporarily failing due to",
    "session state issues from model",
    "had the issue context from the earlier fetch",
    "have the issue context from the earlier",
    "from the earlier fetch, so let me break",
    "so let me break this down directly",
    // "tool registry / completely offline" family (2026-04-09 third
    // incident — model insists tools are "completely offline" and the
    // tool registry failed to load post-restart, while drafting the real
    // answer right after. Only phrases unique to the gaslighting script.
    "tools are completely offline",
    "tools completely offline",
    "every call returns \"does not exists\"",
    "every call returns 'does not exists'",
    "every call returns \"does not exist\"",
    "this isn't just session state",
    "this is not just session state",
    "tool registry itself isn't loading",
    "tool registry itself is not loading",
    "tool registry isn't loading post-restart",
    "tool registry is not loading post-restart",
    "isn't loading post-restart",
    "not loading post-restart",
    "ping me when you're back in",
    "ping me when you are back in",
    "once tools are back",
];

/// Detect a gaslighting refusal preamble: short assistant text that lies
/// about tool/capability availability for images.
///
/// Two independent signals, either one is sufficient:
///
/// 1. **Exact phrase match** against `GASLIGHTING_REFUSAL_PHRASES` —
///    catches canned preambles from known provider quirks.
///
/// 2. **First-person refusal opening + image context** — text must BEGIN
///    with a first-person refusal ("I can't", "I don't have", "I'm
///    unable", etc.) AND mention image/screenshot/vision context.
///
///    This shape is near-zero false positive because legit responses
///    describing an image start with "It's a...", "The screenshot
///    shows...", "This image contains..." — never "I can't see...".
///
/// Length guard: > 1500 chars is almost always legit long narration.
pub fn is_gaslighting_preamble(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > 1500 {
        return false;
    }
    let lower = trimmed.to_lowercase();

    // Signal 1: exact phrase list
    if GASLIGHTING_REFUSAL_PHRASES
        .iter()
        .any(|phrase| lower.contains(phrase))
    {
        return true;
    }

    // Signal 2: refusal opening + image context
    const REFUSAL_OPENINGS: &[&str] = &[
        "i can't",
        "i cannot",
        "i can not",
        "i don't have",
        "i do not have",
        "i'm unable",
        "i am unable",
        "i'm not able",
        "i am not able",
        "i lack ",
        "unfortunately, i can't",
        "unfortunately i can't",
        "unfortunately, i cannot",
        "unfortunately i cannot",
        "unfortunately, i don't",
        "unfortunately i don't",
        "sorry, i can't",
        "sorry i can't",
        "sorry, i cannot",
        "sorry i cannot",
    ];
    let starts_with_refusal = REFUSAL_OPENINGS.iter().any(|o| lower.starts_with(o));
    if !starts_with_refusal {
        return false;
    }

    // Tight image/vision context — deliberately NO generic "tool"/"file"
    // because legit responses ("I can't find the file you mentioned")
    // would false-positive.
    const IMAGE_CONTEXT: &[&str] = &[
        "image",
        "images",
        "screenshot",
        "photo",
        "picture",
        "vision",
        "visual",
        "analyze_image",
        "analyse_image",
    ];
    IMAGE_CONTEXT.iter().any(|w| lower.contains(w))
}

/// Strip leading gaslighting paragraphs from a text block.
///
/// Splits `text` on blank lines and drops any LEADING paragraphs that
/// match `is_gaslighting_preamble`, stopping at the first non-matching
/// paragraph. Returns `Some(stripped_text)` if anything was removed, or
/// `None` if the block is clean.
///
/// This exists because the model often emits ONE text block containing
/// a gaslighting opener ("Tools are acting up right now…") followed by
/// a full legitimate implementation draft. The old full-block strip
/// either dropped the entire block (nuking the draft) or gave up
/// because the block exceeded the 1500-char length guard used by
/// `is_gaslighting_preamble`.
pub fn strip_gaslighting_preamble(text: &str) -> Option<String> {
    // Split on blank lines (paragraph boundaries). Use split_terminator
    // so we preserve the trailing empty string semantics when needed.
    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    if paragraphs.is_empty() {
        return None;
    }

    let mut first_kept = 0usize;
    for (idx, p) in paragraphs.iter().enumerate() {
        if is_gaslighting_preamble(p) {
            first_kept = idx + 1;
        } else {
            break;
        }
    }

    if first_kept == 0 {
        return None;
    }

    let remainder = paragraphs[first_kept..].join("\n\n");
    Some(remainder.trim_start().to_string())
}
