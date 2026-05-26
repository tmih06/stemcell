//! End-to-end test for the auto-title flow.
//!
//! Issue #118 + #120 + #121: Telegram sessions were stuck on their
//! default channel-generated titles forever. Multiple "fixes" shipped
//! without this test. Each one looked correct on code review but did
//! NOT actually change the title in DB after a real message went
//! through. The reporter kept reproducing on his install while local
//! tests reported "fixed".
//!
//! This test simulates the exact flow:
//! 1. Create a session with the default Telegram-style title
//!    (matches `is_default_channel_title` so auto-title is allowed).
//! 2. Run a real user message through
//!    `send_message_with_tools_and_mode`, the same entry point the
//!    Telegram handler uses.
//! 3. Poll the DB until the background auto-title task lands the
//!    rewritten title (bounded by a short timeout so a hung task
//!    doesn't deadlock the suite).
//! 4. Assert (a) the title actually changed, (b) the [chat:N] suffix
//!    was preserved, (c) the `Telegram: ` channel prefix was preserved.
//!
//! If this test fails, the auto-title fix is broken. Period.

use crate::brain::agent::service::AgentService;
use crate::brain::provider::types::ContentDelta;
use crate::brain::provider::{
    ContentBlock, LLMRequest, LLMResponse, Provider, ProviderStream, Role, StopReason, StreamEvent,
    StreamMessage, TokenUsage,
};
use crate::brain::tools::ToolRegistry;
use crate::db::Database;
use crate::services::{ServiceContext, SessionService};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::{Duration, Instant};

// Both scenarios are folded into a SINGLE `#[tokio::test]` because
// `Database::GLOBAL_POOL` is a `OnceLock` set by the first
// `Database::connect_in_memory()` call. The second test in the same
// suite inherits a pool pointing at the first test's torn-down DB,
// producing flaky `Failed to create message` errors. Running both
// scenarios in one runtime keeps the global-pool reference stable
// across the two phases.

/// Provider that returns the same canned response for both `stream` (used
/// by the main user-message turn) and `complete` (used by the auto-title
/// background task). The text the title generator gets is "Greeting" so
/// the resulting session title becomes `Telegram: Greeting [chat:N]` and
/// the test can assert against that exact shape.
struct AutoTitleMockProvider;

#[async_trait]
impl Provider for AutoTitleMockProvider {
    async fn complete(&self, _request: LLMRequest) -> crate::brain::provider::Result<LLMResponse> {
        // The title-generation call uses .complete() with the canned
        // "Generate a concise session title..." prompt. Return a clean
        // 1-word title.
        Ok(LLMResponse {
            id: "title-resp".to_string(),
            model: "mock-model".to_string(),
            content: vec![ContentBlock::Text {
                text: "Greeting".to_string(),
            }],
            stop_reason: Some(StopReason::EndTurn),
            usage: TokenUsage {
                input_tokens: 50,
                output_tokens: 1,
                ..Default::default()
            },
        })
    }

    async fn stream(&self, _request: LLMRequest) -> crate::brain::provider::Result<ProviderStream> {
        // The main user-message turn streams a short text response. No
        // tool calls — we just want the turn to complete cleanly so the
        // background auto-title task gets a chance to run.
        let events = vec![
            Ok(StreamEvent::MessageStart {
                message: StreamMessage {
                    id: "main-resp".to_string(),
                    model: "mock-model".to_string(),
                    role: Role::Assistant,
                    usage: TokenUsage {
                        input_tokens: 100,
                        output_tokens: 0,
                        ..Default::default()
                    },
                },
            }),
            Ok(StreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlock::Text {
                    text: String::new(),
                },
            }),
            Ok(StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "Hey!".to_string(),
                },
            }),
            Ok(StreamEvent::ContentBlockStop { index: 0 }),
            Ok(StreamEvent::MessageDelta {
                delta: crate::brain::provider::MessageDelta {
                    stop_reason: Some(StopReason::EndTurn),
                    stop_sequence: None,
                },
                usage: TokenUsage {
                    input_tokens: 100,
                    output_tokens: 1,
                    ..Default::default()
                },
            }),
            Ok(StreamEvent::MessageStop),
        ];
        Ok(Box::pin(futures::stream::iter(events)))
    }

    fn name(&self) -> &str {
        "auto-title-mock"
    }

    fn default_model(&self) -> &str {
        "mock-model"
    }

    fn supported_models(&self) -> Vec<String> {
        vec!["mock-model".to_string()]
    }

    fn context_window(&self, _model: &str) -> Option<u32> {
        Some(200_000)
    }

    fn calculate_cost(&self, _model: &str, _input: u32, _output: u32) -> f64 {
        0.0
    }
}

/// Helper that runs the full auto-title round-trip against an
/// arbitrary mock provider and returns the rewritten title (or None
/// if the title never changed within 3s).
async fn run_auto_title_round_trip(
    provider: Arc<dyn Provider>,
    initial_title: &str,
) -> Option<String> {
    let db = Database::connect_in_memory().await.unwrap();
    db.run_migrations().await.unwrap();
    let context = ServiceContext::new(db.pool().clone());

    let registry = ToolRegistry::new();
    let agent_service = AgentService::new_for_test(provider, context.clone())
        .await
        .with_tool_registry(Arc::new(registry))
        .with_auto_approve_tools(true);

    let session_service = SessionService::new(context.clone());
    let session = session_service
        .create_session(Some(initial_title.to_string()))
        .await
        .unwrap();
    let session_id = session.id;

    let _ = agent_service
        .send_message_with_tools_and_mode(session_id, "Hi".to_string(), None, None)
        .await
        .expect("first turn should complete");

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let s = session_service
            .get_session(session_id)
            .await
            .unwrap()
            .unwrap();
        if let Some(t) = s.title.as_deref()
            && t != initial_title
        {
            return Some(t.to_string());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

#[tokio::test]
async fn auto_title_end_to_end_covers_text_and_thinking_only_responses() {
    // Phase 1 — Normal provider with Text response.
    let default_title = "Telegram: DM TestUser (133526395) [chat:133526395]";
    let title = run_auto_title_round_trip(Arc::new(AutoTitleMockProvider), default_title)
        .await
        .expect(
            "Phase 1: auto-title task did not update the session title within 3s. \
             The Text-block path is broken (issue #121 — multiple v0.3.x releases \
             claimed to fix this without ever actually testing the end-to-end flow).",
        );
    assert!(
        title.starts_with("Telegram: "),
        "Phase 1: channel prefix must be preserved, got: {title:?}",
    );
    assert!(
        title.ends_with("[chat:133526395]"),
        "Phase 1: [chat:ID] suffix must be preserved (issue #115), got: {title:?}",
    );
    assert!(
        title.contains("Greeting"),
        "Phase 1: must include the LLM-generated body, got: {title:?}",
    );

    // Phase 2 — Reasoning model that returns ONLY Thinking, no Text.
    // This was the actual smoking gun behind @leshchenko1979's
    // reproduction. Before the `extract_title_candidate` fallback,
    // this path returned an empty title and the session stayed stuck
    // on the default channel-generated name forever.
    let default_title_2 = "Telegram: DM Алексей (133526395) [chat:133526395]";
    let title2 = run_auto_title_round_trip(Arc::new(ThinkingOnlyTitleProvider), default_title_2)
        .await
        .expect(
            "Phase 2: auto-title must extract a candidate from the Thinking block \
             when no Text block is present. Reasoning models like \
             qwen-3.7-max-preview-thinking sometimes return Thinking only for \
             short prompts — that's the exact failure mode hit on the reporter's box.",
        );
    assert!(
        title2.starts_with("Telegram: "),
        "Phase 2: channel prefix must be preserved, got: {title2:?}",
    );
    assert!(
        title2.ends_with("[chat:133526395]"),
        "Phase 2: chat suffix must be preserved, got: {title2:?}",
    );
}

/// Provider that mimics qwen-3.7-max-preview-thinking on a short prompt:
/// the streamed response (main user turn) emits text fine, but the
/// `complete()` call (used by auto-title) returns ONLY a `Thinking`
/// block — no `Text` block. That's the exact failure mode on
/// @leshchenko1979's setup behind issue #121.
struct ThinkingOnlyTitleProvider;

#[async_trait]
impl Provider for ThinkingOnlyTitleProvider {
    async fn complete(&self, _request: LLMRequest) -> crate::brain::provider::Result<LLMResponse> {
        // No Text block — only Thinking. extract_text_from_response
        // ignores Thinking blocks and returns "", which is the root
        // cause of the auto-title silent-failure loop.
        Ok(LLMResponse {
            id: "title-thinking-resp".to_string(),
            model: "qwen-thinking-mock".to_string(),
            content: vec![ContentBlock::Thinking {
                thinking: "User said 'Hi'. I should generate a short title. \
                           Maybe 'Greeting' or 'Casual Chat Start'."
                    .to_string(),
                signature: None,
            }],
            stop_reason: Some(StopReason::EndTurn),
            usage: TokenUsage {
                input_tokens: 50,
                output_tokens: 30,
                ..Default::default()
            },
        })
    }

    async fn stream(&self, _request: LLMRequest) -> crate::brain::provider::Result<ProviderStream> {
        // Main turn streams a normal text reply.
        let events = vec![
            Ok(StreamEvent::MessageStart {
                message: StreamMessage {
                    id: "main-resp".to_string(),
                    model: "qwen-thinking-mock".to_string(),
                    role: Role::Assistant,
                    usage: TokenUsage::default(),
                },
            }),
            Ok(StreamEvent::ContentBlockStart {
                index: 0,
                content_block: ContentBlock::Text {
                    text: String::new(),
                },
            }),
            Ok(StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::TextDelta {
                    text: "Hey!".to_string(),
                },
            }),
            Ok(StreamEvent::ContentBlockStop { index: 0 }),
            Ok(StreamEvent::MessageDelta {
                delta: crate::brain::provider::MessageDelta {
                    stop_reason: Some(StopReason::EndTurn),
                    stop_sequence: None,
                },
                usage: TokenUsage {
                    input_tokens: 100,
                    output_tokens: 1,
                    ..Default::default()
                },
            }),
            Ok(StreamEvent::MessageStop),
        ];
        Ok(Box::pin(futures::stream::iter(events)))
    }

    fn name(&self) -> &str {
        "qwen-thinking-mock"
    }

    fn default_model(&self) -> &str {
        "qwen-thinking-mock"
    }

    fn supported_models(&self) -> Vec<String> {
        vec!["qwen-thinking-mock".to_string()]
    }

    fn context_window(&self, _model: &str) -> Option<u32> {
        Some(200_000)
    }

    fn calculate_cost(&self, _model: &str, _input: u32, _output: u32) -> f64 {
        0.0
    }
}
