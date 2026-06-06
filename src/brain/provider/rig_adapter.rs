use super::error::{ProviderError, Result};
use super::r#trait::{Provider, ProviderStream};
use super::types::*;
use async_trait::async_trait;
use futures::StreamExt;
use rig_core::OneOrMany;
use rig_core::client::CompletionClient;
use rig_core::completion::request::ToolDefinition;
use rig_core::completion::{
    CompletionModel, CompletionRequest,
};
use rig_core::completion::message::{
    AssistantContent, DocumentSourceKind, Image, ImageMediaType, Message as RigMessage, Reasoning,
    Text, ToolCall, ToolFunction, ToolResult, ToolResultContent, UserContent,
};
use std::sync::Arc;

pub struct RigAdapter<C> {
    pub name: String,
    pub default_model: String,
    pub supported_models: Vec<String>,
    pub client_builder: Arc<dyn Fn() -> C + Send + Sync>,
    pub context_window_fn: Option<Arc<dyn Fn(&str) -> Option<u32> + Send + Sync>>,
    pub calculate_cost_fn: Option<Arc<dyn Fn(&str, u32, u32) -> f64 + Send + Sync>>,
    pub base_url: Option<String>,
    /// Optional vision-capable model. When set, `supports_vision()`
    /// returns true so the channel side knows it can route image
    /// attachments through this provider.
    pub vision_model: Option<String>,
}

fn parse_image_media_type(mime: &str) -> Option<ImageMediaType> {
    match mime.to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" => Some(ImageMediaType::JPEG),
        "image/png" => Some(ImageMediaType::PNG),
        "image/gif" => Some(ImageMediaType::GIF),
        "image/webp" => Some(ImageMediaType::WEBP),
        "image/heic" => Some(ImageMediaType::HEIC),
        "image/heif" => Some(ImageMediaType::HEIF),
        "image/svg+xml" => Some(ImageMediaType::SVG),
        _ => None,
    }
}

fn image_from_source(source: &ImageSource) -> Image {
    match source {
        ImageSource::Base64 { media_type, data } => Image {
            data: DocumentSourceKind::Base64(data.clone()),
            media_type: parse_image_media_type(media_type),
            detail: None,
            additional_params: None,
        },
        ImageSource::Url { url } => Image {
            data: DocumentSourceKind::Url(url.clone()),
            media_type: None,
            detail: None,
            additional_params: None,
        },
    }
}

fn build_rig_message(msg: &Message) -> Option<RigMessage> {
    match msg.role {
        Role::System => Some(RigMessage::system(
            msg.content
                .iter()
                .filter_map(|c| {
                    if let ContentBlock::Text { text } = c {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
        )),
        Role::User => {
            let blocks: Vec<UserContent> = msg
                .content
                .iter()
                .filter_map(|c| match c {
                    ContentBlock::Text { text } => {
                        Some(UserContent::Text(Text::new(text.clone())))
                    }
                    ContentBlock::Image { source } => {
                        Some(UserContent::Image(image_from_source(source)))
                    }
                    ContentBlock::ToolResult {
                        tool_use_id, content, ..
                    } => Some(UserContent::ToolResult(ToolResult {
                        id: tool_use_id.clone(),
                        call_id: None,
                        content: OneOrMany::one(ToolResultContent::Text(Text::new(content.clone()))),
                    })),
                    _ => None,
                })
                .collect();
            if blocks.is_empty() {
                None
            } else {
                OneOrMany::many(blocks).ok().map(|content| RigMessage::User { content })
            }
        }
        Role::Assistant => {
            let blocks: Vec<AssistantContent> = msg
                .content
                .iter()
                .filter_map(|c| match c {
                    ContentBlock::Text { text } => {
                        Some(AssistantContent::Text(Text::new(text.clone())))
                    }
                    ContentBlock::Thinking {
                        thinking,
                        signature,
                    } => Some(AssistantContent::Reasoning(Reasoning::new_with_signature(
                        &thinking,
                        signature.clone(),
                    ))),
                    ContentBlock::ToolUse { id, name, input } => Some(
                        AssistantContent::ToolCall(ToolCall::new(id.clone(), ToolFunction::new(
                            name.clone(),
                            input.clone(),
                        ))),
                    ),
                    ContentBlock::Image { source } => {
                        Some(AssistantContent::Image(image_from_source(source)))
                    }
                    _ => None,
                })
                .collect();
            if blocks.is_empty() {
                None
            } else {
                OneOrMany::many(blocks)
                    .ok()
                    .map(|content| RigMessage::Assistant {
                        id: None,
                        content,
                    })
            }
        }
    }
}

fn build_history(request: &LLMRequest) -> OneOrMany<RigMessage> {
    let mut history: Vec<RigMessage> = Vec::new();
    for msg in &request.messages {
        if let Some(rig_msg) = build_rig_message(msg) {
            history.push(rig_msg);
        }
    }
    if history.is_empty() {
        history.push(RigMessage::user(" "));
    }
    OneOrMany::many(history).unwrap_or_else(|_| OneOrMany::one(RigMessage::user(" ")))
}

fn build_tools(request: &LLMRequest) -> Vec<ToolDefinition> {
    let Some(tools) = request.tools.as_ref() else {
        return vec![];
    };
    tools
        .iter()
        .map(|t| ToolDefinition {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: t.input_schema.clone(),
        })
        .collect()
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

        let history = build_history(&request);
        let tools = build_tools(&request);

        let req = CompletionRequest {
            model: None,
            chat_history: history,
            preamble: request.system,
            temperature: request.temperature.map(|t| t as f64),
            max_tokens: request.max_tokens.map(|t| t as u64),
            tools,
            additional_params: None,
            documents: vec![],
            output_schema: None,
            tool_choice: None,
        };

        let res = model
            .completion(req)
            .await
            .map_err(|e| ProviderError::ApiError {
                status: 500,
                message: e.to_string(),
                error_type: None,
            })?;

        let text = res
            .choice
            .into_iter()
            .filter_map(|c| match c {
                AssistantContent::Text(t) => Some(t.text),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        let response_id = res.message_id.unwrap_or_else(|| "rig-response".into());

        Ok(LLMResponse {
            id: response_id,
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

        let history = build_history(&request);
        let tools = build_tools(&request);

        let req = CompletionRequest {
            model: None,
            chat_history: history,
            preamble: request.system,
            temperature: request.temperature.map(|t| t as f64),
            max_tokens: request.max_tokens.map(|t| t as u64),
            tools,
            additional_params: None,
            documents: vec![],
            output_schema: None,
            tool_choice: None,
        };

        let stream_res = model
            .stream(req)
            .await
            .map_err(|e| ProviderError::StreamError(e.to_string()))?;
        let model_name = request.model.clone();

        let mut inside_think = false;
        let mut active_close_tag = 0;
        let mut bytes_consumed = 0;
        let mut carry = String::new();
        let mut block_open = false;

        let event_stream = stream_res
            .map(move |chunk_res| match chunk_res {
                Ok(chunk) => match chunk {
                    rig_core::streaming::StreamedAssistantContent::Text(t) => {
                        let (filtered_text, reasoning_text) =
                            crate::brain::provider::streaming_utils::filter_think_tags(
                                &t.text,
                                &mut inside_think,
                                &mut active_close_tag,
                                &mut bytes_consumed,
                                &mut carry,
                            );

                        let mut events = vec![];
                        if !block_open {
                            events.push(Ok(StreamEvent::ContentBlockStart {
                                index: 0,
                                content_block: ContentBlock::Text {
                                    text: String::new(),
                                },
                            }));
                            block_open = true;
                        }
                        if !reasoning_text.is_empty() {
                            events.push(Ok(StreamEvent::ContentBlockDelta {
                                index: 0,
                                delta: ContentDelta::ReasoningDelta {
                                    text: reasoning_text,
                                },
                            }));
                        }
                        if !filtered_text.is_empty() {
                            events.push(Ok(StreamEvent::ContentBlockDelta {
                                index: 0,
                                delta: ContentDelta::TextDelta {
                                    text: filtered_text,
                                },
                            }));
                        }

                        if events.is_empty() {
                            vec![Ok(StreamEvent::Ping)]
                        } else {
                            events
                        }
                    }
                    rig_core::streaming::StreamedAssistantContent::Reasoning(reasoning) => {
                        let reasoning_text = reasoning
                            .content
                            .iter()
                            .filter_map(|c| match c {
                                rig_core::completion::message::ReasoningContent::Text {
                                    text,
                                    ..
                                } => Some(text.as_str()),
                                rig_core::completion::message::ReasoningContent::Summary(s) => {
                                    Some(s.as_str())
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        let mut events = vec![];
                        if !block_open {
                            events.push(Ok(StreamEvent::ContentBlockStart {
                                index: 0,
                                content_block: ContentBlock::Thinking {
                                    thinking: String::new(),
                                    signature: None,
                                },
                            }));
                            block_open = true;
                        }
                        if !reasoning_text.is_empty() {
                            events.push(Ok(StreamEvent::ContentBlockDelta {
                                index: 0,
                                delta: ContentDelta::ReasoningDelta {
                                    text: reasoning_text,
                                },
                            }));
                        }
                        events
                    }
                    rig_core::streaming::StreamedAssistantContent::ReasoningDelta {
                        reasoning,
                        ..
                    } => {
                        let mut events = vec![];
                        if !block_open {
                            events.push(Ok(StreamEvent::ContentBlockStart {
                                index: 0,
                                content_block: ContentBlock::Thinking {
                                    thinking: String::new(),
                                    signature: None,
                                },
                            }));
                            block_open = true;
                        }
                        events.push(Ok(StreamEvent::ContentBlockDelta {
                            index: 0,
                            delta: ContentDelta::ReasoningDelta {
                                text: reasoning,
                            },
                        }));
                        events
                    }
                    rig_core::streaming::StreamedAssistantContent::Final(res) => {
                        use rig_core::completion::GetTokenUsage;
                        let mut events = vec![];

                        if block_open {
                            events.push(Ok(StreamEvent::ContentBlockStop { index: 0 }));
                            block_open = false;
                        }

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
                    }
                    _ => vec![Ok(StreamEvent::Ping)],
                },
                Err(e) => {
                    let mut events = vec![Ok(StreamEvent::Error {
                        error: e.to_string(),
                    })];
                    if block_open {
                        events.push(Ok(StreamEvent::ContentBlockStop { index: 0 }));
                        block_open = false;
                    }
                    events
                }
            })
            .flat_map(futures::stream::iter);

        let start_event = futures::stream::once(async move {
            Ok(StreamEvent::MessageStart {
                message: StreamMessage {
                    id: "rig-stream".into(),
                    model: model_name,
                    role: Role::Assistant,
                    usage: TokenUsage::default(),
                },
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

    fn supports_vision(&self) -> bool {
        self.vision_model.is_some()
    }
}
