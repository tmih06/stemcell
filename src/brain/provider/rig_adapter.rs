use super::error::{ProviderError, Result};
use super::r#trait::{Provider, ProviderStream};
use super::types::*;
use async_trait::async_trait;
use rig_core::client::CompletionClient;
use rig_core::completion::{CompletionModel, CompletionRequest, Message as RigMessage};
use std::sync::Arc;
use futures::StreamExt;

pub struct RigAdapter<C> {
    pub name: String,
    pub default_model: String,
    pub supported_models: Vec<String>,
    pub client_builder: Arc<dyn Fn() -> C + Send + Sync>,
    pub context_window_fn: Option<Arc<dyn Fn(&str) -> Option<u32> + Send + Sync>>,
    pub calculate_cost_fn: Option<Arc<dyn Fn(&str, u32, u32) -> f64 + Send + Sync>>,
    pub base_url: Option<String>,
}

#[async_trait]
impl<C> Provider for RigAdapter<C>
where
    C: CompletionClient + Send + Sync,
    C::CompletionModel: Send + Sync,
    <C::CompletionModel as CompletionModel>::Response: Send + Sync,
    <C::CompletionModel as CompletionModel>::StreamingResponse: Send + Sync + 'static,
{
    async fn complete(&self, request: LLMRequest) -> Result<LLMResponse> {
        let client = (self.client_builder)();
        let model = client.completion_model(&request.model);

        let mut history = vec![];
        for msg in &request.messages {
            let content_str = msg.content.iter().filter_map(|c| {
                if let ContentBlock::Text { text } = c {
                    Some(text.clone())
                } else {
                    None
                }
            }).collect::<Vec<String>>().join("\n");
            
            match msg.role {
                Role::User => history.push(RigMessage::user(content_str)),
                Role::Assistant => history.push(RigMessage::assistant(content_str)),
                Role::System => history.push(RigMessage::user(format!("System: {}", content_str))),
            }
        }
        if history.is_empty() {
            history.push(RigMessage::user(" "));
        }

        let req = CompletionRequest {
            model: None,
            chat_history: rig_core::OneOrMany::many(history).unwrap(),
            preamble: request.system,
            temperature: request.temperature.map(|t| t as f64),
            max_tokens: request.max_tokens.map(|t| t as u64),
            tools: vec![],
            additional_params: None,
            documents: vec![],
            output_schema: None,
            tool_choice: None,
        };

        let res = model.completion(req).await.map_err(|e| ProviderError::ApiError {
            status: 500,
            message: e.to_string(),
            error_type: None,
        })?;

        let text = res.choice.into_iter().filter_map(|c| {
            match c {
                rig_core::message::AssistantContent::Text(t) => Some(t.text),
                _ => None
            }
        }).collect::<Vec<_>>().join("\n");

        Ok(LLMResponse {
            id: res.message_id.unwrap_or_else(|| "rig-response".into()),
            model: request.model,
            content: vec![ContentBlock::Text { text }],
            stop_reason: Some(StopReason::EndTurn),
            usage: TokenUsage::default(),
            streaming_active_secs: None,
        })
    }

    async fn stream(&self, request: LLMRequest) -> Result<ProviderStream> {
        let client = (self.client_builder)();
        let model = client.completion_model(&request.model);

        let mut history = vec![];
        for msg in &request.messages {
            let content_str = msg.content.iter().filter_map(|c| {
                if let ContentBlock::Text { text } = c {
                    Some(text.clone())
                } else {
                    None
                }
            }).collect::<Vec<String>>().join("\n");
            
            match msg.role {
                Role::User => history.push(RigMessage::user(content_str)),
                Role::Assistant => history.push(RigMessage::assistant(content_str)),
                Role::System => history.push(RigMessage::user(format!("System: {}", content_str))),
            }
        }
        if history.is_empty() {
            history.push(RigMessage::user(" "));
        }

        let req = CompletionRequest {
            model: None,
            chat_history: rig_core::OneOrMany::many(history).unwrap(),
            preamble: request.system,
            temperature: request.temperature.map(|t| t as f64),
            max_tokens: request.max_tokens.map(|t| t as u64),
            tools: vec![],
            additional_params: None,
            documents: vec![],
            output_schema: None,
            tool_choice: None,
        };

        let stream_res = model.stream(req).await.map_err(|e| ProviderError::StreamError(e.to_string()))?;
        let model_name = request.model.clone();

        let mut inside_think = false;
        let mut active_close_tag = 0;
        let mut bytes_consumed = 0;
        let mut carry = String::new();

        let event_stream = stream_res.map(move |chunk_res| {
            match chunk_res {
                Ok(chunk) => {
                    match chunk {
                        rig_core::streaming::StreamedAssistantContent::Text(t) => {
                            let (filtered_text, reasoning_text) = crate::brain::provider::streaming_utils::filter_think_tags(
                                &t.text,
                                &mut inside_think,
                                &mut active_close_tag,
                                &mut bytes_consumed,
                                &mut carry
                            );

                            let mut events = vec![];
                            if !reasoning_text.is_empty() {
                                events.push(Ok(StreamEvent::ContentBlockDelta {
                                    index: 0,
                                    delta: ContentDelta::ReasoningDelta { text: reasoning_text }
                                }));
                            }
                            if !filtered_text.is_empty() {
                                events.push(Ok(StreamEvent::ContentBlockDelta {
                                    index: 0,
                                    delta: ContentDelta::TextDelta { text: filtered_text }
                                }));
                            }

                            if events.is_empty() {
                                vec![Ok(StreamEvent::Ping)]
                            } else {
                                events
                            }
                        },
                        rig_core::streaming::StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                            vec![Ok(StreamEvent::ContentBlockDelta {
                                index: 0,
                                delta: ContentDelta::ReasoningDelta { text: reasoning }
                            })]
                        },
                        rig_core::streaming::StreamedAssistantContent::Final(res) => {
                            use rig_core::completion::GetTokenUsage;
                            let mut events = vec![];
                            
                            let mut opencrabs_usage = TokenUsage::default();
                            if let Some(usage) = res.token_usage() {
                                opencrabs_usage.input_tokens = usage.input_tokens as u32;
                                opencrabs_usage.output_tokens = usage.output_tokens as u32;
                            }
                            
                            events.push(Ok(StreamEvent::MessageDelta {
                                delta: MessageDelta {
                                    stop_reason: Some(StopReason::EndTurn),
                                    stop_sequence: None,
                                },
                                usage: opencrabs_usage,
                            }));
                            events.push(Ok(StreamEvent::MessageStop));
                            events
                        },
                        _ => vec![Ok(StreamEvent::Ping)],
                    }
                },
                Err(e) => {
                    vec![Ok(StreamEvent::Error { error: e.to_string() })]
                }
            }
        }).flat_map(futures::stream::iter);

        let start_event = futures::stream::once(async move {
            Ok(StreamEvent::MessageStart {
                message: StreamMessage {
                    id: "rig-stream".into(),
                    model: model_name,
                    role: Role::Assistant,
                    usage: TokenUsage::default(),
                }
            })
        });

        let combined = start_event.chain(event_stream);

        Ok(Box::pin(combined))
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    fn supported_models(&self) -> Vec<String> {
        self.supported_models.clone()
    }

    fn context_window(&self, model: &str) -> Option<u32> {
        self.context_window_fn.as_ref().and_then(|f| f(model))
    }

    fn calculate_cost(&self, model: &str, input_tokens: u32, output_tokens: u32) -> f64 {
        self.calculate_cost_fn
            .as_ref()
            .map(|f| f(model, input_tokens, output_tokens))
            .unwrap_or(0.0)
    }

    fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }
}
