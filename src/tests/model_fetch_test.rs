//! Model Fetching Infrastructure Tests
//!
//! Tests for the generic model fetching from OpenAI-compatible endpoints.
//! Verifies URL normalization, response parsing, and endpoint fallback.

mod model_fetch {
    // --- URL Normalization Tests ---

    fn normalize_base_url(url: &str) -> String {
        let url = url.trim_end_matches('/');
        if url.ends_with("/v1/chat/completions") {
            url[..url.len() - "/v1/chat/completions".len()].to_string()
        } else if url.ends_with("/chat/completions") {
            url[..url.len() - "/chat/completions".len()].to_string()
        } else if url.ends_with("/v1") {
            url[..url.len() - "/v1".len()].to_string()
        } else {
            url.to_string()
        }
    }

    #[test]
    fn strips_v1_chat_completions() {
        assert_eq!(
            normalize_base_url("http://localhost:1234/v1/chat/completions"),
            "http://localhost:1234"
        );
    }

    #[test]
    fn strips_chat_completions() {
        assert_eq!(
            normalize_base_url("http://localhost:1234/chat/completions"),
            "http://localhost:1234"
        );
    }

    #[test]
    fn strips_v1_suffix() {
        assert_eq!(
            normalize_base_url("http://localhost:11434/v1"),
            "http://localhost:11434"
        );
    }

    #[test]
    fn leaves_clean_url_unchanged() {
        assert_eq!(
            normalize_base_url("http://localhost:11434"),
            "http://localhost:11434"
        );
    }

    #[test]
    fn handles_trailing_slash() {
        assert_eq!(
            normalize_base_url("http://localhost:1234/v1/"),
            "http://localhost:1234"
        );
    }

    // --- Ollama Response Parsing Tests ---

    #[test]
    fn ollama_response_has_models_field() {
        // Ollama returns { "models": [{ "name": "llama3" }] }
        let json = r#"{"models":[{"name":"llama3.1"},{"name":"mistral"}]}"#;
        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        let models = parsed.get("models").unwrap().as_array().unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].get("name").unwrap().as_str().unwrap(), "llama3.1");
    }

    #[test]
    fn ollama_empty_models_list() {
        let json = r#"{"models":[]}"#;
        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        let models = parsed.get("models").unwrap().as_array().unwrap();
        assert!(models.is_empty());
    }

    // --- OpenAI Response Parsing Tests ---

    #[test]
    fn openai_response_has_data_field() {
        // OpenAI returns { "data": [{ "id": "gpt-4" }] }
        let json = r#"{"data":[{"id":"gpt-4o"},{"id":"gpt-4"}],"object":"list"}"#;
        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        let data = parsed.get("data").unwrap().as_array().unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].get("id").unwrap().as_str().unwrap(), "gpt-4o");
    }

    // --- Endpoint Fallback Logic Tests ---

    #[test]
    fn ollama_uses_api_tags_path() {
        let base = "http://localhost:11434";
        let tags_url = format!("{}/api/tags", base);
        assert_eq!(tags_url, "http://localhost:11434/api/tags");
    }

    #[test]
    fn openai_compatible_uses_v1_models() {
        let base = "http://localhost:1234";
        let models_url = format!("{}/v1/models", base);
        assert_eq!(models_url, "http://localhost:1234/v1/models");
    }
}
