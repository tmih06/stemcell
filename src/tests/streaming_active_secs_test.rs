//! Tests that `stream_complete` correctly populates
//! `LLMResponse.streaming_active_secs` with the wall time spent
//! receiving content deltas (with idle gaps >1s excluded). This
//! value flows up to `AgentResponse.tokens_per_second` as the tok/s
//! denominator, replacing the previous full-turn-wall-clock divisor
//! that silently halved the displayed rate on every tool-heavy turn.
//!
//! Live integration of the full tok/s wire-up (provider tokens /
//! summed iteration active windows) is exercised by real-provider
//! runs; here we pin the streaming layer's accumulation in isolation
//! with a controllable-timing mock.

use crate::brain::provider::{
    ContentBlock, ContentDelta, LLMRequest, LLMResponse, Message, MessageDelta, Provider,
    ProviderError, ProviderStream, Role, StopReason, StreamEvent, StreamMessage, TokenUsage,
};
use crate::tests::agent_service_mocks::create_test_service_with_provider;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Mock that emits N text deltas with a fixed delay between each.
/// Lets the test pin the expected active streaming window.
struct TimedDeltaProvider {
    chunks: Vec<String>,
    delay_between_chunks: Duration,
}

impl TimedDeltaProvider {
    fn new(chunks: Vec<&str>, delay_between_chunks: Duration) -> Self {
        Self {
            chunks: chunks.into_iter().map(String::from).collect(),
            delay_between_chunks,
        }
    }
}

#[async_trait]
impl Provider for TimedDeltaProvider {
    async fn complete(
        &self,
        _request: LLMRequest,
    ) -> Result<LLMResponse, ProviderError> {
        unreachable!("test uses stream(), not complete()");
    }

    async fn stream(
        &self,
        _request: LLMRequest,
    ) -> Result<ProviderStream, ProviderError> {
        // Drive a real time gap between deltas by spawning a producer
        // that sleeps and sends through an mpsc channel, then wrap
        // the receiver in a Stream. `futures::stream::iter` would
        // collapse all events into a single poll and erase the
        // intervals — useless for measuring active_secs.
        let chunks = self.chunks.clone();
        let delay = self.delay_between_chunks;
        let total_text: String = chunks.join("");
        let (tx, rx) =
            tokio::sync::mpsc::unbounded_channel::<Result<StreamEvent, ProviderError>>();

        tokio::spawn(async move {
            let _ = tx.send(Ok(StreamEvent::MessageStart {
                message: StreamMessage {
                    id: "timed-resp".into(),
                    model: "mock-timed".into(),
                    role: Role::Assistant,
                    usage: TokenUsage::default(),
                },
            }));
            let _ = tx.send(Ok(StreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlock::Text {
                    text: String::new(),
                },
            }));
            for (i, chunk) in chunks.into_iter().enumerate() {
                if i > 0 {
                    tokio::time::sleep(delay).await;
                }
                if tx
                    .send(Ok(StreamEvent::ContentBlockDelta {
                        index: 0,
                        delta: ContentDelta::TextDelta { text: chunk },
                    }))
                    .is_err()
                {
                    return;
                }
            }
            let _ = tx.send(Ok(StreamEvent::ContentBlockStop { index: 0 }));
            let _ = tx.send(Ok(StreamEvent::MessageDelta {
                delta: MessageDelta {
                    stop_reason: Some(StopReason::EndTurn),
                    stop_sequence: None,
                },
                usage: TokenUsage {
                    output_tokens: total_text.split_whitespace().count() as u32,
                    ..Default::default()
                },
            }));
            let _ = tx.send(Ok(StreamEvent::MessageStop));
        });

        // Wrap the mpsc receiver in a futures Stream via poll_fn so
        // we don't need tokio-stream (not a workspace dep).
        let mut rx = rx;
        let stream = futures::stream::poll_fn(move |cx| rx.poll_recv(cx));
        Ok(Box::pin(stream))
    }

    fn name(&self) -> &str {
        "mock-timed"
    }

    fn default_model(&self) -> &str {
        "mock-timed"
    }

    fn supported_models(&self) -> Vec<String> {
        vec!["mock-timed".into()]
    }

    fn context_window(&self, _: &str) -> Option<u32> {
        Some(4096)
    }

    fn calculate_cost(&self, _: &str, _: u32, _: u32) -> f64 {
        0.0
    }
}

#[tokio::test]
async fn streaming_active_secs_sums_intervals_between_deltas() {
    // 4 chunks with 100ms between each → 3 inter-chunk gaps → ~300ms
    // total active window. The first chunk opens the window at t=0,
    // each subsequent chunk extends it, so window = last - start.
    let provider = Arc::new(TimedDeltaProvider::new(
        vec!["one ", "two ", "three ", "four"],
        Duration::from_millis(100),
    ));
    let (svc, _) = create_test_service_with_provider(provider).await;
    let request = LLMRequest::new("mock-timed".to_string(), vec![Message::user("go")]);

    let (response, _) = svc
        .stream_complete(Uuid::nil(), request, None, None, None, None, false)
        .await
        .expect("stream_complete must succeed");

    let active = response
        .streaming_active_secs
        .expect("streaming_active_secs must be Some when deltas were received");

    // 3 gaps × 100ms = 300ms expected. Allow generous slop for scheduler
    // jitter — the assertion just has to be tight enough to catch a
    // wall-clock-based denominator (which would include ALL turn time
    // including pre-stream handshake).
    assert!(
        (0.25..=0.45).contains(&active),
        "expected ~0.3s active streaming time, got {active:.3}s — \
         wall-clock denominator would produce a significantly larger value, \
         a zero-delta path would produce 0"
    );
}

#[tokio::test]
async fn streaming_active_secs_excludes_long_idle_gaps() {
    // 2 chunks separated by a 1.2s gap (longer than IDLE_GAP_SECS=1.0).
    // The first window closes after the first chunk (zero-width — only
    // one delta). The second window opens at the second chunk (also
    // zero-width). Total active_secs should be near zero, NOT 1.2s.
    //
    // This is the whole point of the active-window measurement: a
    // 30s tool call between two model bursts must NOT pad the
    // denominator. Without the idle-gap filter, the displayed rate
    // would collapse from ~50 tok/s to ~2 tok/s the moment a slow
    // tool ran mid-turn.
    let provider = Arc::new(TimedDeltaProvider::new(
        vec!["before ", "after"],
        Duration::from_millis(1200),
    ));
    let (svc, _) = create_test_service_with_provider(provider).await;
    let request = LLMRequest::new("mock-timed".to_string(), vec![Message::user("go")]);

    let (response, _) = svc
        .stream_complete(Uuid::nil(), request, None, None, None, None, false)
        .await
        .expect("stream_complete must succeed");

    // Expected: ~0s (both windows are single-delta → zero width). The
    // 1.2s gap between them is correctly excluded. Allow up to 100ms
    // for scheduler / per-event overhead.
    match response.streaming_active_secs {
        Some(active) => {
            assert!(
                active < 0.15,
                "active_secs must exclude the 1.2s idle gap; got {active:.3}s. \
                 If this fails near 1.2s, the idle-gap filter regressed and \
                 the tok/s rate will collapse on every tool-heavy turn."
            );
        }
        None => {
            // None is also acceptable here — both windows are
            // effectively zero-width, so total active_secs is 0, and
            // stream_complete returns None for that. The test passes
            // either way: the key invariant is the gap was excluded.
        }
    }
}
