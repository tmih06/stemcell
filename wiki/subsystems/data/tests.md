# Data Layer — Tests

## Test Strategy

- Inline `#[cfg(test)] mod tests` in each repository and memory file
- SQLite tests always run (no external dependencies required)
- Feature-gated integration tests

## Running

| Command | Scope |
|---------|-------|
| `cargo test --all-features` | Full test suite |
| `cargo test -p stemcell -- db` | Database tests only |
| `cargo test -p stemcell -- memory` | Memory tests only |

## Test Areas

| Area | What's tested |
|------|---------------|
| Repository CRUD | Each repo: insert, read, update, delete, edge cases |
| FTS5 | Full-text search indexing and querying |
| Embeddings | Local GGUF and API embedding generation |
| Hybrid search | RRF scoring and result ranking |
| Migrations | Schema versioning, idempotent re-application |
| Retry | Backoff timing, max retries exceeded behavior |

---

**Navigation:** [Index](index.md) | [Source Map](source-map.md) | [Flows](flows.md)
