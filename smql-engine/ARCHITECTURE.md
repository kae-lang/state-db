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
```

## Key Design Decisions

### 1. AST as Foundation
All types live in `smql-ast` with zero internal dependencies. This ensures every crate can work with the type system without circular dependencies.

### 2. Async-First
All public APIs are async even if initially synchronous. This avoids a painful refactor when adding actual I/O.

### 3. Storage Trait
The `Storage` trait is the only I/O boundary. The engine never touches disk directly. This makes backends pluggable (Memory for tests, RocksDB for production).

### 4. Immutable Trail
Trail entries are append-only. Once written, never modified. The trail is the authoritative history of what happened.

### 5. Transitions Are The Only Way
State changes only happen through the transition pipeline: validate → guards → mutate → store → trail → actions. No backdoor state mutations.

### 6. ULID IDs
Instance IDs use ULID format (sortable, unique, timestamp-embedded), prefixed with a machine short identifier.

### 7. Structured Errors
Every error carries type, context, and (where possible) a hint for resolution. No bare string errors in library code.

### 8. No unwrap() in Libraries
All errors propagated with `?`. `unwrap()` only in tests and examples.

## Value Type System

SMQL supports these types:
- **Primitives:** Text, Int, Float, Bool
- **Identity:** Uuid
- **Temporal:** Date, DateTime, Duration
- **Complex:** Enum(variants), List(T), Set(T), Map(K,V)
- **References:** Ref(MachineName)
- **Special:** Money(currency), Blob, Json, Null

## Expression System

Guards and WHERE clauses share the same expression evaluator. Expressions support:
- Binary operations (comparison, arithmetic, logical)
- Field access (dot notation, SELF, ACTOR)
- Function calls (elapsed(), NOW(), etc.)
- Collection predicates (ALL, ANY, COUNT)
- State predicates (STATE IS, STATE IN)
- Null checks (IS SET, IS NOT SET)
