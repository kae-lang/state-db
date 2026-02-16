# Crate Graph & Design Principles

The SMQL engine is organized as a Rust workspace with 12 crates. Each crate has a single responsibility, and dependencies flow in one direction: downstream crates never depend on upstream ones.

## Crate Map

```
smql-ast            Pure types. No logic, no I/O.
  |
smql-parser         Hand-written recursive descent lexer + parser.
  |
smql-catalog        Machine registry backed by DashMap.
  |
smql-storage        Storage trait + MemoryStorage + RocksDB (feature-gated).
  |
smql-timer          TimerManager for timeout transitions.
  |
smql-hooks          HookExecutor + EventBus (tokio::broadcast).
  |
smql-engine-core    The brain: spawn, transition, query, eval, alter.
  |
smql-query          Thin re-exports (query types live in smql-ast).
  |
  +-- smql-server   Axum HTTP/JSON API + WebSocket + Prometheus metrics.
  +-- smql-cli      Clap CLI + rustyline REPL.
  +-- smql-sdk      HTTP client, FindBuilder, Subscription, typed API.
  +-- smql-codegen  Code generation from machine definitions.
```

## What Each Crate Does

| Crate | Purpose |
|---|---|
| `smql-ast` | AST types: `Value`, `Expression`, `Machine`, `Command`, `Query`. Every other crate depends on this. |
| `smql-parser` | Hand-written recursive descent parser. Lexer uppercases keywords; `expect_ident()` preserves original case. Produces `Vec<Statement>`. |
| `smql-catalog` | `MachineCatalog` backed by `DashMap<String, Machine>`. Concurrent read/write without a global lock. |
| `smql-storage` | The `Storage` trait plus two implementations: `MemoryStorage` (DashMap-based, always available) and `RocksDBStorage` (behind the `rocksdb` feature flag). |
| `smql-timer` | `TimerManager` with a dual-index design (BTreeMap by deadline, HashMap by key) for efficient scheduling and O(1) cancellation. |
| `smql-hooks` | `HookExecutor` runs lifecycle hooks. `EventBus` uses `tokio::broadcast` for EMIT. BEFORE hooks are sync and can reject; everything else is fire-and-forget. |
| `smql-engine-core` | The engine. Owns spawn, transition, query execution, expression evaluation, and ALTER MACHINE. Contains `eval_expr()` directly (not in smql-query) to avoid circular dependencies. |
| `smql-query` | Thin wrapper that re-exports query types from `smql-ast`. Exists for API clarity. |
| `smql-server` | Axum HTTP server. Routes, JSON handlers, WebSocket subscriptions, Prometheus metrics. Metrics live here, not in the engine. |
| `smql-cli` | `clap` for argument parsing, `rustyline` for the interactive REPL. Connects to the server or runs embedded. |
| `smql-sdk` | Rust HTTP client for the SMQL server. `SmqlClient` constructs SMQL strings from builder parameters since the server only accepts `POST /execute` with `{"smql": "..."}`. |
| `smql-codegen` | Generates Rust types from machine definitions. Maps SMQL types to Rust: `TEXT` to `String`, `INT` to `i64`, `MONEY` to `(i64, String)`, etc. |

## Design Principles

### Engine stays dependency-free from infrastructure

`smql-engine-core` has no knowledge of Axum, Prometheus, or any transport layer. Metrics are instrumented in the server handlers. Timeout metrics go through an EventBus subscriber in the server, not by coupling the engine to prometheus. This keeps the engine testable and reusable.

### Hand-written parser for control and diagnostics

There is no parser combinator library. The lexer and parser are written by hand as a recursive descent parser. This gives full control over error messages, keyword handling, and syntax extensions. Keywords are uppercased during lexing so matching is case-insensitive, but identifiers preserve their original casing.

### DashMap for lock-free concurrency

The catalog and memory storage both use `DashMap` from the `dashmap` crate. Each shard in a `DashMap` can be accessed independently, which means concurrent reads and writes to different keys never block each other. No global mutex, no RwLock around the whole map.

### Arc-based sharing throughout

The `Engine` holds `Arc<dyn Storage>`, `Arc<MachineCatalog>`, and other shared resources. The server's `AppState` is `Clone` because every field is an `Arc`. This makes it trivial to pass state into Axum handlers and tokio tasks.

### Feature flags for optional backends

RocksDB is gated behind the `rocksdb` feature in `smql-storage`, `smql-server`, and `smql-cli`. A build without this feature compiles faster and has no native dependency on librocksdb. The `MemoryStorage` is always available.

### Map literals and SPAWN in the AST

Map literals `{key: value}` are represented as a `__map` FunctionCall node in the AST. Similarly, `SPAWN Machine {}` inside a MUTATE clause becomes a `__spawn` FunctionCall. The engine detects `__spawn` before calling `eval_expr()` because spawn is an async operation that cannot be handled inside the synchronous expression evaluator.

## Dependency Flow

The critical rule: **dependencies flow downward**. `smql-ast` depends on nothing. `smql-parser` depends on `smql-ast`. `smql-engine-core` depends on most lower crates. `smql-server` and `smql-cli` depend on `smql-engine-core` but never the reverse.

```
smql-server ─┐
smql-cli ────┤
smql-sdk ────┴──► smql-engine-core ──► smql-hooks
smql-codegen                       ──► smql-timer
                                   ──► smql-storage ──► smql-catalog ──► smql-ast
                                                                     ──► smql-parser
```

This means you can use the engine as a library without pulling in Axum or clap. You can use the parser without pulling in the engine. You can use the AST types without pulling in anything.
