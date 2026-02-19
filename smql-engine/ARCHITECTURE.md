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

New top-level catalog entries (stored in `smql-catalog`):
- `PolicyDefinition` — named reusable guard bundles
- `RuleDefinition` — cross-instance invariants
- `ViewDefinition` — named live FIND queries
- `ProjectionDefinition` — named materialized AGGREGATE queries
- `SubscriptionDefinition` — declarative event-to-action routing
- `SagaDefinition` — multi-machine orchestration workflows

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
The expression evaluator lives in `smql-engine-core` (not `smql-query`) to avoid circular dependencies. Guards, WHERE clauses, COMPUTED fields, REACTIVE conditions, and RULE guards all share the same evaluator.

### 10. Recursive Async with Box::pin
Recursive async transitions (THROUGH multi-hop) use `Box::pin` with explicit lifetimes to satisfy the compiler.

### 11. Engine Stays Dependency-Free
The core engine has no dependency on prometheus, axum, or any server concern. Metrics are instrumented at the server layer via `SmqlMetrics` stored in `AppState`.

### 12. Callback Architecture
`EngineCallbackImpl` holds cloned `Arc` fields (not `Arc<Engine>`) to avoid self-referential Arc. `HookExecutor.callback` uses `RwLock` for post-construction setting via `&self`. `engine.wire_callback()` MUST be called after `Engine::new()` in every entry point.

### 13. Computed Fields Are Post-Write
Computed fields are evaluated **after** the primary data write (spawn or WITH mutation) and written in the same atomic batch. The evaluator receives the already-mutated field map, so computed fields can reference each other if declared in dependency order. Computed fields are stripped from incoming write payloads before validation.

### 14. Reactive Rules Are Post-Commit
REACTIVE rules are evaluated after every successful transition commit (including child transitions that signal upward). Evaluation is async and non-blocking — a reactive `TRY TRANSITION` that fails guard checks is silently discarded. Loop detection compares `(instance_id, target_state)` against the current call stack.

### 15. Saga Executor Is Async and Detached
Saga steps execute in a detached `tokio::spawn` task after the triggering transition commits. Each step is a normal `engine.transition()` call. Compensation runs in reverse step order using the same path. Saga state (current step, status) is stored in the catalog under the saga instance ID.

### 16. Rule Engine Runs Before Guard Evaluation
Rules registered for `BEFORE SPAWN` or `BEFORE TRANSITION` run before the transition's own guards. All rule guards are evaluated; all failures are collected into a `Vec<RuleViolated>` and returned as a single error. `AFTER TRANSITION` rules run post-commit as fire-and-forget.

### 17. Subscription Router Is Event-Driven
The subscription router listens on the same `tokio::broadcast` channel as the EventBus. On each event, it matches against registered subscriptions by machine name, event type, and state. Matched subscriptions execute their actions asynchronously (fire-and-forget). Subscriptions survive restarts because they are persisted in the catalog.

### 18. Projections Are Cached in Storage
Projection results are serialized and stored in the `projections` RocksDB column family (key: projection name). `REFRESH ON TRANSITION` projections are re-computed inside the post-commit hook. `REFRESH ON INTERVAL` projections are managed by a background `tokio::interval` task registered at startup. `REFRESH MANUAL` projections are only recomputed on explicit request.

### 19. Field-Level ACL Is Applied at Read and Write Boundaries
On write: computed fields and fields in `CANNOT WRITE` / outside `CAN WRITE` are stripped from the incoming payload before validation. Violations produce `WritePermissionDenied`. On read: field filtering is applied to the serialized `Value::Map` after the instance is fetched, before the response is serialized. The `AS "role"` clause on GET/FIND passes the role name through the read path.

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
7 column families: `instances`, `state_index`, `machine_index`, `trails`, `parent_index`, `id_index`, `projections`.
- `projections` CF: key = projection name (UTF-8), value = serde_json serialized aggregate result
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

### Dwell Timers

`ON DWELL` hooks use the same `TimerManager` with a distinct `TimerKind::Dwell` variant. Key differences from timeout timers:
- **Repeating:** after firing, the dwell timer re-registers itself with the same deadline offset from `now()` (not from the original state entry time). This produces the "every N duration" repeat behaviour.
- **No state transition:** dwell timer callbacks invoke `HookExecutor::run_dwell_hook()` instead of `engine.transition()`.
- **Cancelled on state exit:** the transition pipeline calls `timer_manager.cancel_dwell(instance_id, state)` before committing the new state.
- **Multiple dwell hooks per state:** each `ON DWELL(state, > duration)` clause registers a separate timer entry with a unique key `{instance_id}:{state}:{duration_ms}`.

## Hook Architecture

Hooks are declared in the machine's `HOOKS` block with braces around trigger body (no `DO` keyword):

```
HOOKS {
  ON SPAWN { ACTION : EMIT("created") }
  BEFORE EACH TRANSITION { ACTION : LOG("transitioning") }
  ON DWELL(in_progress, > 48h) { ACTION : NOTIFY(assignee, "ticket.stale") }
}
```

- BEFORE hooks are synchronous and can reject (treated as guard failure)
- AFTER hooks and all other hooks are async (fire-and-forget)
- ON DWELL hooks are timer-driven (see Dwell Timers above)
- EventBus uses `tokio::broadcast` for EMIT
- Engine pre-resolves `Action` → `ResolvedAction` (concrete Values) before passing to HookExecutor
- `Action::Conditional { condition, action }` is evaluated by the hook executor: if the condition is false, the inner action is skipped silently

## Composition Architecture

- Instance has `parent_id` / `parent_machine` fields
- MemoryStorage has `parent_index` DashMap for efficient child lookups
- `__spawn` in MUTATE detected at engine level (before `eval_expr`) since spawn is async
- ALL over empty children = true (vacuous truth), ANY over empty = false
- CASCADE recursively transitions children to first terminal state (if guard fails, child stays)
- After every child transition, the engine calls `reactive_engine.evaluate(parent_id)` to check REACTIVE rules

## Reactive Engine Architecture

The `ReactiveEngine` holds a reference to the catalog and the engine callback. After each successful transition:
1. Fetch the instance's machine definition.
2. If the machine has a `REACTIVE` block, evaluate each `ReactiveClause.condition` against the current instance context.
3. For each condition that evaluates to `true`, call `engine.try_transition(instance_id, target_state)` (guard-evaluated, non-fatal).
4. Loop detection: maintain a `HashSet<(instance_id, target_state)>` per call stack frame; skip if already attempted in this chain.
5. Reactive transitions execute as `Actor::System`.

## Policy Catalog

`PolicyDefinition` entries are stored in the catalog keyed by name. At transition evaluation time, `APPLY POLICY` clauses are resolved by name and their `guards: Vec<Expression>` are prepended to the transition's own guard list. Unknown policy names produce a `PolicyNotFound` error at registration time (not at runtime).

## Rule Engine Architecture

`RuleDefinition` entries are stored in the catalog indexed by `(machine_name, trigger)`. The rule engine is invoked:
- Before spawn: `rule_engine.check(machine_name, RuleTrigger::BeforeSpawn, context)`
- Before transition: `rule_engine.check(machine_name, RuleTrigger::BeforeTransition, context)`
- After transition: `rule_engine.check_async(machine_name, RuleTrigger::AfterTransition, context)` (fire-and-forget)

All `BEFORE` rule guards are evaluated eagerly; all failures are collected into `Vec<RuleViolated>` and returned as a single `SmqlError::RuleViolations`.

## Subscription Router Architecture

`SubscriptionRouter` holds a `Vec<SubscriptionDefinition>` loaded from the catalog at startup. It subscribes to the engine's `tokio::broadcast` event channel. On each event:
1. Match `event.machine_name` and `event.kind` (enter/exit/spawn/transition) against each subscription's trigger.
2. For matching subscriptions, spawn a `tokio::task` to execute the subscription's actions.
3. Wildcard `FROM *` / `TO *` are represented as `None` in the `SubscriptionEvent` trigger fields.

New subscriptions registered at runtime are appended to the in-memory list and persisted to the catalog.

## Saga Executor Architecture

The `SagaExecutor` is wired into the subscription router — sagas are triggered by the same event mechanism as subscriptions. On trigger:
1. A new saga instance record is created in the catalog (status: `Running`, current_step: 0).
2. Steps execute sequentially in a detached `tokio::spawn` task.
3. Each step: evaluate optional `WHEN` condition; if false, skip. Otherwise call `engine.transition()`.
4. On step failure: collect succeeded steps in reverse order, run each step's `COMPENSATE` transition (using `TRY` semantics — compensation failure is logged, not fatal).
5. On complete/failure: execute `ON COMPLETE` / `ON FAILURE` actions, update saga instance status.

Saga instance state is stored in the catalog under key `saga:{saga_name}:{trigger_instance_id}`.

## HTTP Server Architecture

axum-based with routes:
- `POST /execute` — parse and execute any SMQL statement
- `GET /machines`, `GET /machines/:name` — machine registry
- `GET /instances/:id` — instance lookup
- `GET /health`, `GET /metrics` — operational endpoints
- `GET /subscribe` — WebSocket event streaming with query params

Auth: JWT HS256 via `jsonwebtoken`, feature-gated behind `auth`. `/health` and `/metrics` bypass auth.
