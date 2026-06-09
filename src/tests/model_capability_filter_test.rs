//! The chat TUI is the main interface, so the model picker must only surface
//! models capable of conversation + tool use. These tests pin the two filters
//! that strip non-conversational bloat (embedding / image / audio / rerank /
//! …) from the warmed model cache.

use crate::startup::jobs::fetch_models::is_conversational_model_type;
use crate::startup::model_cache::is_chat_capable_model_id;

#[test]
fn modeldb_keeps_chat_and_responses_types() {
    assert!(is_conversational_model_type(Some("chat")));
    // OpenAI GPT-5 / o-series are served via the Responses API; the `chatgpt`
    // provider exposes only this type and is fully chat + tool-use capable.
    assert!(is_conversational_model_type(Some("responses")));
}

#[test]
fn modeldb_drops_non_conversational_types() {
    for t in [
        "image",
        "embedding",
        "audio",
        "rerank",
        "video_generation",
        "ocr",
        "moderation",
        "image_edit",
        "search",
        "vector_store",
        "realtime",
        // Legacy text completion predates the chat/tool-use contract.
        "completion",
    ] {
        assert!(
            !is_conversational_model_type(Some(t)),
            "model_type {t:?} must be filtered out of the chat picker"
        );
    }
}

#[test]
fn modeldb_missing_type_is_kept() {
    // A missing model_type must not silently empty a provider's list.
    assert!(is_conversational_model_type(None));
}

#[test]
fn id_filter_keeps_chat_models() {
    for id in [
        "gpt-5.5",
        "gpt-4o",
        "o3",
        "o4-mini",
        "chatgpt-4o-latest",
        "gpt-4.1",
        "claude-opus-4-8",
        "gemini-2.5-pro",
        "MiniMax-M3",
        "anthropic.claude-sonnet-4-v1:0",
        "qwen3-max",
        "glm-4.6",
        // Multimodal *chat* models that accept image input — their ids carry
        // no image-generation token, so they must stay.
        "gpt-4o",
        "gpt-4-vision-preview",
        "gemini-2.5-flash",
    ] {
        assert!(
            is_chat_capable_model_id(id),
            "{id} is a chat model and must stay in the picker"
        );
    }
}

#[test]
fn id_filter_drops_non_chat_models() {
    for id in [
        // OpenAI
        "dall-e-3",
        "dall-e-2",
        "whisper-1",
        "tts-1",
        "tts-1-hd",
        "text-embedding-3-large",
        "text-embedding-ada-002",
        "text-moderation-latest",
        "omni-moderation-latest",
        "gpt-image-1",
        "sora-2",
        "babbage-002",
        "davinci-002",
        "gpt-4o-audio-preview",
        "gpt-4o-realtime-preview",
        "gpt-4o-transcribe",
        "gpt-4o-mini-tts",
        "gpt-4o-search-preview",
        "computer-use-preview",
        // Bedrock / cross-provider
        "cohere.embed-english-v3",
        "amazon.titan-embed-text-v2:0",
        "cohere.rerank-v3-5:0",
        "amazon.rerank-v1:0",
        // Google / Vertex
        "imagen-3.0-generate-002",
        "imagen-4.0-ultra-generate-001",
        "veo-2.0-generate-001",
        "text-embedding-004",
        "multimodalembedding@001",
        "gemini-embedding-001",
        "imagegeneration@006",
        "gemini-2.5-flash-image",
        "gemini-3-pro-image-preview",
        "gemini-2.0-flash-exp-image-generation",
        // Qwen / Bedrock image + diffusion
        "qwen-image-2.0-pro",
        "amazon.nova-canvas-v1:0",
        "stability.stable-image-ultra-v1:1",
        "stability.sd3-5-large-v1:0",
        "stability.stable-fast-upscale-v1:0",
    ] {
        assert!(
            !is_chat_capable_model_id(id),
            "{id} is not a chat/tool-use model and must be filtered out"
        );
    }
}
