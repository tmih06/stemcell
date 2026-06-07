pub const STRIP_OPEN_TAGS: &[&str] = &["<think>", "<!-- reasoning -->", "<!--"];
pub const STRIP_CLOSE_TAGS: &[&[&str]] = &[
    &["</think>"],
    &["-->"],
    &["-->"], // Generic HTML comments
];
const MAX_OPEN_TAG_CARRY: usize = 17;
const THINK_BLOCK_MAX_BYTES: usize = 200_000;

/// Filters think tags from streaming output and extracts reasoning content.
pub fn filter_think_tags(
    text: &str,
    inside_think: &mut bool,
    active_close_tag: &mut usize,
    bytes_consumed: &mut usize,
    carry: &mut String,
) -> (String, String) {
    let mut owned: String;
    let input_ref: &str = if carry.is_empty() {
        text
    } else {
        owned = std::mem::take(carry);
        owned.push_str(text);
        owned.as_str()
    };
    let mut result = String::new();
    let mut reasoning = String::new();
    let mut remaining = input_ref;

    let is_reasoning_block = |idx: usize| idx < 2;

    loop {
        if *inside_think {
            *bytes_consumed += remaining.len();
            if *bytes_consumed > THINK_BLOCK_MAX_BYTES {
                tracing::warn!(
                    "⚠️ Think-tag filter consumed {} bytes without close tag \
                     (tag_idx={}) — still waiting for close, continuing to suppress",
                    *bytes_consumed,
                    *active_close_tag,
                );
                if is_reasoning_block(*active_close_tag) {
                    reasoning.push_str(remaining);
                }
                *bytes_consumed = 0;
                break;
            }

            let close_candidates = STRIP_CLOSE_TAGS[*active_close_tag];
            let earliest_close = close_candidates
                .iter()
                .filter_map(|close| remaining.find(close).map(|pos| (pos, *close)))
                .min_by_key(|(pos, _)| *pos);

            if let Some((end, close)) = earliest_close {
                if is_reasoning_block(*active_close_tag) {
                    reasoning.push_str(&remaining[..end]);
                }
                remaining = &remaining[end + close.len()..];
                *inside_think = false;
                *bytes_consumed = 0;
            } else {
                if is_reasoning_block(*active_close_tag) {
                    reasoning.push_str(remaining);
                }
                break;
            }
        } else {
            let mut earliest: Option<(usize, usize)> = None;
            for (i, open) in STRIP_OPEN_TAGS.iter().enumerate() {
                if let Some(pos) = remaining.find(open)
                    && earliest.is_none_or(|(best, _)| pos < best)
                {
                    earliest = Some((pos, i));
                }
            }

            if let Some((pos, tag_idx)) = earliest {
                result.push_str(&remaining[..pos]);
                remaining = &remaining[pos + STRIP_OPEN_TAGS[tag_idx].len()..];
                *inside_think = true;
                *active_close_tag = tag_idx;
                *bytes_consumed = 0;
            } else {
                let tail_keep = open_tag_prefix_len(remaining);
                if tail_keep > 0 {
                    let split_at = remaining.len() - tail_keep;
                    result.push_str(&remaining[..split_at]);
                    carry.push_str(&remaining[split_at..]);
                } else {
                    result.push_str(remaining);
                }
                break;
            }
        }
    }

    (result, reasoning)
}

pub fn tool_marker_prefix_len(s: &str, markers: &[&str]) -> usize {
    let max_marker_len = markers.iter().map(|m| m.len()).max().unwrap_or(0);
    if max_marker_len <= 1 {
        return 0;
    }
    let start = s.len().saturating_sub(max_marker_len - 1);
    for i in start..s.len() {
        if !s.is_char_boundary(i) {
            continue;
        }
        let suffix = &s[i..];
        if suffix.is_empty() {
            continue;
        }
        if markers
            .iter()
            .any(|m| m.len() > suffix.len() && m.starts_with(suffix))
        {
            return suffix.len();
        }
    }
    0
}

fn open_tag_prefix_len(s: &str) -> usize {
    let tail_starts = s
        .char_indices()
        .map(|(i, _)| i)
        .filter(|i| s.len() - i <= MAX_OPEN_TAG_CARRY);
    for start in tail_starts {
        let suffix = &s[start..];
        for open in STRIP_OPEN_TAGS {
            if open.len() > suffix.len() && open.starts_with(suffix) {
                return suffix.len();
            }
        }
    }
    0
}

pub fn strip_think_blocks(text: &str) -> String {
    let mut result = text.to_string();
    for (open, close_candidates) in STRIP_OPEN_TAGS.iter().zip(STRIP_CLOSE_TAGS.iter()) {
        while let Some(start) = result.find(open) {
            let earliest_close = close_candidates
                .iter()
                .filter_map(|close| result[start..].find(close).map(|end| (end, *close)))
                .min_by_key(|(end, _)| *end);

            if let Some((end, close)) = earliest_close {
                result = format!(
                    "{}{}",
                    &result[..start],
                    &result[start + end + close.len()..]
                );
            } else {
                result = result[..start].to_string();
                break;
            }
        }
    }
    result
}
