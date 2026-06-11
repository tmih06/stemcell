# Data Layer — Flows

## DB Connection Flow

```
Startup → Config → Database::new()
  → deadpool-sqlite pool creation
  → rusqlite_migration::apply()
  → Database ready
```

## Migration Flow

```
Startup → rusqlite_migration::Migrator::new()
  → load all .sql files from src/migrations/
  → compare with PRAGMA user_version
  → apply pending migrations in order
  → update user_version
```

## CRUD Flow

```
Service layer → Repository trait method
  → Connection::get() from deadpool pool
  → rusqlite prepared statement / query
  → Result → Repository → Service
```

## Memory Indexing Flow

```
Input text → MemoryEngine::index()
  → Tokenize → FTS5 INSERT
  → Compute embedding vector (local GGUF or API)
  → Store vector + metadata
```

## Memory Search Flow

```
Query → MemoryEngine::search()
  ├── FTS5 full-text search → score A
  └── Vector similarity search → score B
  → Reciprocal Rank Fusion (RRF): combined_score = 1/(k + rank_A) + 1/(k + rank_B)
  → Sort by combined score → return top-K results
```

## Retry Flow

```
DB operation → rusqlite error
  → retry.rs: exponential backoff (e.g. 50ms, 100ms, 200ms, ...)
  → max retries exceeded → propagate error to caller
```

---

**Navigation:** [Index](index.md) | [Source Map](source-map.md) | [Tests](tests.md)
