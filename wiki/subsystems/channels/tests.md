# Channels — Tests

## Test Files

| File | Scope |
|------|-------|
| `src/channels/tests.rs` | Core channel integration tests |
| Per-channel `tests` modules | Inline tests within each channel module |

## Running Tests

```bash
# All channels (requires all feature flags)
cargo test --features telegram,discord,slack,whatsapp,trello --test channels

# Single channel
cargo test --features telegram

# All features (includes local-stt, local-tts)
cargo test --all-features

# Voice tests
cargo test --features local-stt,local-tts -p stemcell voice
```

## Test Patterns

### Feature-gated tests

All channel tests are behind `#[cfg(feature = "...")]` guards. A channel with its feature disabled produces zero test code.

### Mockito-based HTTP tests (Trello)

Trello uses pure HTTP (no external SDK), so its tests use **mockito** for HTTP mocking:

```rust
// Example pattern (conceptual)
#[cfg(test)]
mod tests {
    use mockito::Server;

    #[test]
    fn test_trello_create_card() {
        let mut server = Server::new();
        let mock = server.mock("POST", "/1/cards")
            .with_status(200)
            .with_body(r#"{"id": "abc123", "name": "Test Card"}"#)
            .create();

        let client = TrelloClient::new(&server.url(), "key", "token");
        let card = client.create_card("list_id", "Test Card").unwrap();
        mock.assert();
    }
}
```

### Channel integration tests

`tests.rs` exercises the full `ChannelFactory` → `ChannelManager` pipeline with mocked configs to verify connection lifecycle, greeting generation, and session init/resolve paths.

## Continuous Integration

CI runs with `--all-features` on push/PR to ensure no channel regresses. See `.github/workflows/` for the current matrix.
