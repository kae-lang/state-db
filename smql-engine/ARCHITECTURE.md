# SMQL Engine — Architecture

> Living document. Updated as design decisions are made.

## Overview

SMQL Engine is a state machine database written in Rust. It provides a domain-specific language (SMQL) for defining state machines, spawning instances, transitioning between states (with guards and actions), and querying instance state and history.

## Crate Dependency Graph

```
smql-ast          (zero deps — core types)
  ↑
smql-parser       (depends on: ast)
smql-catalog      (depends on: ast)
smql-storage      (depends on: ast)
smql-trail        (depends on: ast)
smql-timer        (depends on: ast)
smql-hooks        (depends on: ast)
smql-query        (depends on: ast, storage)
  ↑
smql-engine-core  (depends on: ast, catalog, storage, trail, timer, hooks)
  ↑
smql-server       (depends on: ast, engine-core, parser, storage)
smql-cli          (depends on: ast, parser)
smql-sdk          (depends on: ast)
smql-codegen      (depends on: ast, parser)
```

13 crates in workspace.

## Key Design Decisions

### 1. AST as Foundation
All types live in `smql-ast` with zero internal dependencies. This ensures every crate can work with the type system without circular dependencies.

### 2. Async-First
All public APIs are async even if initially synchronous. This avoids a painful refactor when adding actual I/O.

### 3. Storage Trait
The `Storage` trait is the only I/O boundary. The engine never touches disk directly. This makes backends pluggable (Memory for tests, RocksDB for production).

### 4. Immutable Trail
Trail entries are append-only. Once written, never modified. The trail is the authoritative history of what happened. Trail entries include a spawn event at sequence 0 (from_state is empty).

### 5. Transitions Are The Only Way
State changes only happen through the transition pipeline: validate → guards → mutate → store → trail → actions. No backdoor state mutations.

### 6. ULID IDs
Instance IDs use ULID format (26 characters, sortable, unique, timestamp-embedded). ULIDs enable efficient cursor-based pagination via natural sort order.

### 7. Structured Errors
Every error carries type, context, and (where possible) a hint for resolution. No bare string errors in library code. `SmqlError::ValidationError` includes an optional `field` for context.

### 8. No unwrap() in Libraries
All errors propagated with `?`. `unwrap()` only in tests and examples.

### 9. Expression Evaluator in Engine
The expression evaluator lives in `smql-engine-core` (not `smql-query`) to avoid circular dependencies. Guards and WHERE clauses share the same evaluator.

### 10. Recursive Async with Box::pin
Recursive async transitions (THROUGH multi-hop) use `Box::pin` with explicit lifetimes to satisfy the compiler.

### 11. Engine Stays Dependency-Free
The core engine has no dependency on prometheus, axum, or any server concern. Metrics are instrumented at the server layer via `SmqlMetrics` stored in `AppState`.

### 12. Callback Architecture
`EngineCallbackImpl` holds cloned `Arc` fields (not `Arc<Engine>`) to avoid self-referential Arc. `HookExecutor.callback` uses `RwLock` for post-construction setting via `&self`. `engine.wire_callback()` MUST be called after `Engine::new()` in every entry point.

## Value Type System

SMQL supports these types:
- **Primitives:** Text, Int, Float, Bool
- **Identity:** Uuid
- **Temporal:** Date, DateTime, Duration
- **Complex:** Enum(variants), List(T), Set(T), Map(K,V)
- **References:** Ref(MachineName)
- **Special:** Money(currency), Blob, Json, Null

**Important:** `Value::Map` uses `BTreeMap` (not `HashMap`). Money/Ref types don't compare with Int — guards like `total > 0` fail if total is `Money(9999, "USD")`.

## Expression System

Guards and WHERE clauses share the same expression evaluator. Expressions support:
- Binary operations (comparison, arithmetic, logical)
- Field access (dot notation, SELF, ACTOR)
- Function calls: `elapsed()`, `elapsed_in_state()`, `elapsed_since()`, `NOW()`, `TODAY()`, `timeout_remaining()`, `len()`, `lower()`, `upper()`, `count()`
- Collection predicates: `ALL` (vacuous truth for empty), `ANY` (false for empty), `COUNT`
- State predicates: `STATE IS`, `STATE IN`
- Null checks: `IS SET`, `IS NOT SET`, `IS NULL`
- Signal predicates: `SIGNAL FROM Machine WHERE condition`
- Map literals: `{ key: value }` → parsed as `__map` FunctionCall in AST

## Storage Architecture

### Memory Storage
DashMap-based concurrent storage with separate indices for state, machine, parent, and id lookups.

### RocksDB Storage (feature-gated)
6 column families: `instances`, `state_index`, `machine_index`, `trails`, `parent_index`, `id_index`.
- Composite keys with NUL (`\x00`) separator
- WriteBatch for atomic multi-write operations
- Range iteration with upper bounds (not prefix_iterator_cf)
- serde_json serialization

## Timer Architecture

`TimerManager` uses a two-index design:
- `BTreeMap<deadline, Vec<entry>>` — efficient "what fires next?" lookups
- `HashMap<key, deadline>` — O(1) cancel by instance_id+state

Timeout transitions bypass guards (guard-free, executed as System actor). Timers are persisted via write-through to storage and restored on startup via `restore_timers()`.

Timer storage key: `{instance_id}:{state}` (memory) or `{instance_id}\0{state}` (RocksDB).

## Hook Architecture

Hooks are declared in the machine's `HOOKS` block with braces around trigger body (no `DO` keyword):

```
HOOKS {
  ON SPAWN { ACTION : EMIT("created") }
  BEFORE EACH TRANSITION { ACTION : LOG("transitioning") }
}
```

- BEFORE hooks are synchronous and can reject (treated as guard failure)
- AFTER hooks and all other hooks are async (fire-and-forget)
- EventBus uses `tokio::broadcast` for EMIT
- Engine pre-resolves `Action` → `ResolvedAction` (concrete Values) before passing to HookExecutor

## Composition Architecture

- Instance has `parent_id` / `parent_machine` fields
- MemoryStorage has `parent_index` DashMap for efficient child lookups
- `__spawn` in MUTATE detected at engine level (before `eval_expr`) since spawn is async
- ALL over empty children = true (vacuous truth), ANY over empty = false
- CASCADE recursively transitions children to first terminal state (if guard fails, child stays)

## HTTP Server Architecture

axum-based with routes:
- `POST /execute` — parse and execute any SMQL statement
- `GET /machines`, `GET /machines/:name` — machine registry
- `GET /instances/:id` — instance lookup
- `GET /health`, `GET /metrics` — operational endpoints
- `GET /subscribe` — WebSocket event streaming with query params

Auth: JWT HS256 via `jsonwebtoken`, feature-gated behind `auth`. `/health` and `/metrics` bypass auth.
