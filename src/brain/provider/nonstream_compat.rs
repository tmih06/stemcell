//! Compatibility shim: synthesize Anthropic-style `StreamEvent`s from a
//! non-streaming OpenAI-compatible chat-completion JSON response.
//!
//! Some upstream proxies / local models return a full `chat.completion`
//! JSON object when StemCell asks for streaming (e.g. when `stream=true`
//! is ignored or unsupported). The streaming tool loop expects a sequence
//! of `StreamEvent`s. Rather than re-implementing the whole response
//! pipeline, we convert the JSON into the same event sequence that the
//! streaming path would have produced.
//!
//! The event ordering mirrors the Anthropic Messages API streaming
//! format used elsewhere in the codebase:
//!
//! ```text
//! MessageStart
//! ContentBlockStart         (one per block: text, tool, ...)
//! ContentBlockDelta         (zero or more per block)
//! ContentBlockStop
//! MessageDelta              (carries final usage + stop_reason)
//! MessageStop
//! ```

use crate::brain::provider::types::{
    ContentBlock, ContentDelta, MessageDelta, Role, StopReason, StreamEvent, StreamMessage,
    TokenUsage,
};
use serde_json::Value;

/// Returns `true` when the given body looks like a non-streaming
/// `chat.completion` response (i.e. `object == "chat.completion"`, not
/// `"chat.completion.chunk"` and not wrapped in `data: ...` SSE).
pub fn is_nonstream_response(body: &str) -> bool {
    let trimmed = body.trim_start();
    if trimmed.starts_with("data:") || !trimmed.starts_with('{') {
        return false;
    }
    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        return false;
    };
    matches!(
        value.get("object").and_then(|v| v.as_str()),
        Some("chat.completion")
    )
}

/// Convert a non-streaming OpenAI-style chat-completion JSON body into the
/// sequence of `StreamEvent`s the streaming pipeline would have produced.
pub fn synthesize_stream_events(
    body: &str,
) -> Result<Vec<Result<StreamEvent, ()>>, serde_json::Error> {
    let value: Value = serde_json::from_str(body)?;
    let mut events: Vec<Result<StreamEvent, ()>> = Vec::new();

    let id = value
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let model = value
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let usage = parse_usage(value.get("usage"));

    // Pick the first choice (proxy responses always have one for non-streaming).
    let choice = value
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first());

    let (finish_reason, message) = match choice {
        Some(c) => (
            c.get("finish_reason")
                .and_then(|v| v.as_str())
                .unwrap_or("stop")
                .to_string(),
            c.get("message").cloned().unwrap_or(Value::Null),
        ),
        None => ("stop".to_string(), Value::Null),
    };

    // 1. MessageStart
    events.push(Ok(StreamEvent::MessageStart {
        message: StreamMessage {
            id: id.clone(),
            model: model.clone(),
            role: Role::Assistant,
            usage,
        },
    }));

    let stop_reason = map_stop_reason(&finish_reason);

    // 2. Optional reasoning delta (some providers carry it in `message.reasoning`).
    if let Some(reasoning) = message.get("reasoning").and_then(|v| v.as_str())
        && !reasoning.is_empty()
    {
        events.push(Ok(StreamEvent::ContentBlockDelta {
            index: 0,
            delta: ContentDelta::ReasoningDelta {
                text: reasoning.to_string(),
            },
        }));
    }

    // 3. Optional text content
    let content_text = message
        .get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.trim_start_matches('\n').to_string());
    let mut next_index: usize = if message.get("reasoning").is_some() {
        1
    } else {
        0
    };
    if let Some(text) = content_text
        && !text.is_empty()
    {
        let idx = next_index;
        next_index += 1;
        events.push(Ok(StreamEvent::ContentBlockStart {
            index: idx,
            content_block: ContentBlock::Text { text: text.clone() },
        }));
        events.push(Ok(StreamEvent::ContentBlockStop { index: idx }));
    }

    // 4. Tool calls
    if let Some(tool_calls) = message.get("tool_calls").and_then(|v| v.as_array()) {
        for tc in tool_calls {
            let call_id = tc
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = tc
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let arguments_str = tc
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|v| v.as_str())
                .unwrap_or("{}");
            let input: serde_json::Value =
                serde_json::from_str(arguments_str).unwrap_or(Value::Null);

            let idx = next_index;
            next_index += 1;
            events.push(Ok(StreamEvent::ContentBlockStart {
                index: idx,
                content_block: ContentBlock::ToolUse {
                    id: call_id,
                    name,
                    input,
                },
            }));
            events.push(Ok(StreamEvent::ContentBlockStop { index: idx }));
        }
    }

    // 5. MessageDelta (carries final stop_reason + usage)
    events.push(Ok(StreamEvent::MessageDelta {
        delta: MessageDelta {
            stop_reason: Some(stop_reason),
            stop_sequence: None,
        },
        usage,
    }));

    // 6. MessageStop
    events.push(Ok(StreamEvent::MessageStop));

    Ok(events)
}

fn map_stop_reason(s: &str) -> StopReason {
    match s {
        "stop" | "end_turn" => StopReason::EndTurn,
        "length" | "max_tokens" => StopReason::MaxTokens,
        "tool_calls" | "tool_use" => StopReason::ToolUse,
        "stop_sequence" => StopReason::StopSequence,
        _ => StopReason::EndTurn,
    }
}

fn parse_usage(value: Option<&Value>) -> TokenUsage {
    let Some(v) = value else {
        return TokenUsage::default();
    };
    let input_tokens = v
        .get("prompt_tokens")
        .and_then(|x| x.as_u64())
        .map(|n| n as u32)
        .unwrap_or(0);
    let output_tokens = v
        .get("completion_tokens")
        .and_then(|x| x.as_u64())
        .map(|n| n as u32)
        .unwrap_or(0);
    let cache_creation_tokens = v
        .get("cache_creation_input_tokens")
        .and_then(|x| x.as_u64())
        .map(|n| n as u32)
        .unwrap_or(0);
    let cache_read_tokens = v
        .get("prompt_tokens_details")
        .and_then(|d| d.get("cached_tokens"))
        .and_then(|x| x.as_u64())
        .map(|n| n as u32)
        .unwrap_or(0);
    TokenUsage {
        input_tokens,
        output_tokens,
        cache_creation_tokens,
        cache_read_tokens,
        billing_cache_creation: 0,
        billing_cache_read: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_nonstream_shape() {
        assert!(is_nonstream_response(
            r#"{"object":"chat.completion","id":"x"}"#
        ));
        assert!(!is_nonstream_response(
            r#"data: {"object":"chat.completion"}"#
        ));
        assert!(!is_nonstream_response("not json"));
        assert!(!is_nonstream_response(
            r#"{"object":"chat.completion.chunk"}"#
        ));
    }
}
