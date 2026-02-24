# SMQL Engine — AI Agent Enhancements Checklist

> Last updated: 2026-02-24
> Purpose: Extend SMQL engine to be a first-class state machine database for AI agents
> Prerequisite: All phases 1-15 + bugfixes complete (951+ tests passing)
> Reference: Review analysis performed 2026-02-24 by senior engineer

---

## Overview

This checklist covers 16 enhancement areas organized into 3 tiers by priority.
Each enhancement is designed to make AI agents more efficient and effective when
using SMQL as their state management layer.

**Tier 1 — Critical (Do First):** Core capabilities agents cannot work without
**Tier 2 — High Impact (Do Second):** Major efficiency and reliability improvements
**Tier 3 — Nice to Have (Do Third):** Ecosystem features for advanced agent patterns

---

## Tier 1: Critical Missing Capabilities

### Enhancement 1: Transition Graph Introspection (`EXPLAIN TRANSITIONS`) [STATUS: COMPLETE]

> **Why:** Agents currently operate by trial-and-error — they attempt transitions and
> parse error messages to understand what's possible. This is the single biggest
> efficiency bottleneck. Agents should be able to ask "what can I do from here?"
> before acting.

- [ ] 1.1 AST additions
  - [ ] 1.1.1 Add `ExplainTransitions` variant to `Query` enum in `smql-ast/src/query.rs`
    - Fields: `machine: String`, `instance_id: Option<String>` (None = schema-level, Some = instance-level)
  - [ ] 1.1.2 Add `AvailableTransition` struct to `smql-ast/src/lib.rs`
    - Fields: `from_state: String`, `to_state: String`, `guards: Vec<String>` (guard expression strings), `guards_met: bool`, `blocking_guards: Vec<GuardFailure>`, `recovery_options: Vec<RecoveryOption>`, `requires_data: Vec<String>` (fields referenced in guards), `requires_role: Option<String>` (if guard references ACTOR.role)
  - [ ] 1.1.3 Add `ExplainResult` variant to `QueryResult` enum
    - Fields: `current_state: String`, `available: Vec<AvailableTransition>`, `machine: String`, `instance_id: Option<String>`

- [ ] 1.2 Parser additions
  - [ ] 1.2.1 Parse `EXPLAIN TRANSITIONS FOR Machine` (schema-level: returns all transitions grouped by from_state)
  - [ ] 1.2.2 Parse `EXPLAIN TRANSITIONS FOR Machine "instance_id"` (instance-level: evaluates guards against real data)
  - [ ] 1.2.3 Parse `EXPLAIN TRANSITIONS FOR Machine "instance_id" AS "actor"` (with actor context)
  - [ ] 1.2.4 Parser tests: 4 tests (schema, instance, with actor, parse error)

- [ ] 1.3 Engine implementation
  - [ ] 1.3.1 `Engine::explain_transitions_schema(machine: &str)` — returns all transitions from the machine definition with guard expression strings, no evaluation
  - [ ] 1.3.2 `Engine::explain_transitions_instance(machine: &str, instance_id: &str, actor: Option<&str>)`:
    - Load instance and verify machine
    - Filter transitions to those valid from current state (direct, wildcard ANY, group)
    - For each valid transition:
      - Build EvalContext (load children, parent, timers)
      - Evaluate each guard expression, catching failures
      - Set `guards_met: bool` based on whether all guards pass
      - Populate `blocking_guards` with `GuardFailure` details for failed guards
      - Generate `recovery_options` using AST-based analysis (see Enhancement 8)
      - Extract `requires_data` from guard expression field references
      - Extract `requires_role` if guard references ACTOR.role
    - Return sorted: guards_met=true first, then by to_state alphabetically
  - [ ] 1.3.3 Wire into `Engine::execute_query()` match arm
  - [ ] 1.3.4 Engine tests: 6 tests
    - Schema-level explain returns all transitions
    - Instance-level with all guards met
    - Instance-level with some guards failing (blocking_guards populated)
    - Instance-level with actor context (ACTOR.role guards evaluated)
    - Instance in terminal state (empty available list)
    - Instance with timeout active (timeout_remaining in context)

- [ ] 1.4 Server wiring
  - [ ] 1.4.1 Handle `ExplainResult` in `POST /execute` response JSON serialization
  - [ ] 1.4.2 Add `GET /instances/:id/transitions` REST endpoint (convenience shortcut)
    - Query param: `?as=actor_id` for actor context
    - Internally calls `explain_transitions_instance`
  - [ ] 1.4.3 Add `GET /machines/:name/transitions` REST endpoint (schema-level)
    - Optional query param: `?from_state=X` to filter
    - Internally calls `explain_transitions_schema`
  - [ ] 1.4.4 Server tests: 3 tests (POST /execute, GET /instances/:id/transitions, GET /machines/:name/transitions)

- [ ] 1.5 SDK additions
  - [ ] 1.5.1 `SmqlClient::explain_transitions(machine, instance_id, actor)` method
  - [ ] 1.5.2 `ExplainResponse` type with `available_transitions: Vec<AvailableTransition>`
  - [ ] 1.5.3 SDK test: 2 tests (schema-level, instance-level with guard evaluation)

- [ ] 1.6 CHECKPOINT: All existing tests pass + new tests pass

---

### Enhancement 2: Instance Claiming / Distributed Work Queue (`CLAIM`) [STATUS: COMPLETE]

> **Why:** Without exclusive claiming, multi-agent deployments experience thundering
> herd problems. All agents find the same pending instance, all try to transition it,
> N-1 fail with version conflicts and retry — creating load spikes and wasted compute.
> Agents need an atomic "give me one unit of work" primitive.

- [ ] 2.1 Data model additions
  - [ ] 2.1.1 Add to `Instance` struct in `smql-storage/src/instance.rs`:
    - `claimed_by: Option<String>` — agent ID holding the claim
    - `claim_expires_at: Option<DateTime<Utc>>` — lease expiry timestamp
  - [ ] 2.1.2 Add `ClaimCommand` to `smql-ast/src/command.rs`:
    - `machine: String`
    - `filter: Option<Expression>` — WHERE clause
    - `agent_id: String` — who is claiming
    - `lease_duration: SmqlDuration` — how long the claim lasts
    - `transition_to: Option<String>` — optional state to transition to on claim
    - `sort_by: Option<Vec<SortField>>` — ordering for "which instance to claim first"
  - [ ] 2.1.3 Add `ReleaseCommand` to `smql-ast/src/command.rs`:
    - `machine: String`
    - `instance_id: String`
    - `agent_id: String` — must match current claimant
  - [ ] 2.1.4 Add `ClaimResult` struct: `instance_id: String, machine: String, state: String, data: HashMap, claimed_until: DateTime<Utc>`

- [ ] 2.2 Storage layer
  - [ ] 2.2.1 Add `claim_instance(machine: &str, instance_id: &str, agent_id: &str, expires_at: DateTime<Utc>) -> SmqlResult<()>` to Storage trait
    - Atomic: check `claimed_by` is None or expired, then set both fields
    - Return `Conflict` error if already claimed by another agent
  - [ ] 2.2.2 Add `release_claim(instance_id: &str, agent_id: &str) -> SmqlResult<()>` to Storage trait
    - Only release if `claimed_by` matches agent_id
  - [ ] 2.2.3 Add `find_and_claim(machine: &str, filter: Option<&Expression>, agent_id: &str, expires_at: DateTime<Utc>, sort: Option<&[SortField]>) -> SmqlResult<Option<Instance>>` to Storage trait
    - Atomic find-one-and-claim: iterate candidates, skip claimed (non-expired), claim the first match
    - Return None if no unclaimed instance matches
  - [ ] 2.2.4 MemoryStorage implementation (DashMap atomic get_mut for claim check+set)
  - [ ] 2.2.5 RocksDB implementation (TransactionDB with get_for_update for true atomicity)
    - Note: This requires migrating from `DB` to `TransactionDB` in `rocksdb.rs`
    - If TransactionDB migration is too large, use a Mutex-guarded read-check-write as interim
  - [ ] 2.2.6 Storage tests: 8 tests
    - Claim unclaimed instance succeeds
    - Claim already-claimed instance fails with Conflict
    - Claim expired claim succeeds (lease expired)
    - Release claim by correct agent succeeds
    - Release claim by wrong agent fails
    - find_and_claim skips claimed instances
    - find_and_claim returns None when all claimed
    - find_and_claim respects sort order

- [ ] 2.3 Parser additions
  - [ ] 2.3.1 Parse `CLAIM Machine WHERE expr AS "agent-id" LEASE 30s`
  - [ ] 2.3.2 Parse `CLAIM Machine WHERE expr AS "agent-id" LEASE 30s THEN TRANSITION TO state`
  - [ ] 2.3.3 Parse `RELEASE Machine "instance_id" AS "agent-id"`
  - [ ] 2.3.4 Parser tests: 4 tests (basic claim, claim with transition, release, parse errors)

- [ ] 2.4 Engine implementation
  - [ ] 2.4.1 `Engine::claim(cmd: &ClaimCommand) -> SmqlResult<Option<ClaimResult>>`
    - Validate machine exists
    - Call `storage.find_and_claim()` with filter, agent_id, computed expires_at
    - If `transition_to` is set, perform transition after claim (atomic with claim)
    - Register lease expiry timer in TimerManager
    - Return ClaimResult or None
  - [ ] 2.4.2 `Engine::release(cmd: &ReleaseCommand) -> SmqlResult<()>`
    - Call `storage.release_claim()`
    - Cancel lease expiry timer
  - [ ] 2.4.3 Lease expiry handler: when timer fires, clear `claimed_by` and `claim_expires_at`
    - Reuse TimerManager with a new timer key format: `claim:{instance_id}`
  - [ ] 2.4.4 Modify FIND queries: add implicit filter `(claimed_by IS NULL OR claim_expires_at < NOW())` unless `INCLUDE CLAIMED` modifier is present
  - [ ] 2.4.5 Engine tests: 8 tests
    - Basic claim and release flow
    - Claim with WHERE filter
    - Claim with THEN TRANSITION TO
    - Concurrent claims — second agent gets Conflict
    - Lease expiry — instance becomes claimable again
    - FIND excludes claimed instances by default
    - FIND INCLUDE CLAIMED shows claimed instances
    - Claim + transition atomic (if transition fails, claim is not applied)

- [ ] 2.5 Server wiring
  - [ ] 2.5.1 Handle CLAIM in `POST /execute` — return ClaimResult JSON or 404 if none available
  - [ ] 2.5.2 Handle RELEASE in `POST /execute`
  - [ ] 2.5.3 Server tests: 3 tests (claim via HTTP, release via HTTP, claim returns 404 when none available)

- [ ] 2.6 SDK additions
  - [ ] 2.6.1 `SmqlClient::claim(machine, filter, agent_id, lease_duration)` → `SdkResult<Option<ClaimResult>>`
  - [ ] 2.6.2 `SmqlClient::release(machine, instance_id, agent_id)` → `SdkResult<()>`
  - [ ] 2.6.3 SDK tests: 2 tests (claim + release, claim when none available)

- [ ] 2.7 CHECKPOINT: All existing tests pass + new tests pass

---

### Enhancement 3: Idempotency Keys [STATUS: COMPLETE]

> **Why:** Network failures between agent and SMQL server cause ambiguity: "did my
> SPAWN succeed or not?" Without idempotency, agents either create duplicates (retry)
> or lose operations (don't retry). This is a fundamental reliability requirement
> for any system agents interact with over a network.

- [x] 3.1 Data model
  - [x] 3.1.1 Add `idempotency_key: Option<String>` to `SpawnCommand` and `TransitionCommand`
  - [x] 3.1.2 Added idempotency methods to `Storage` trait: `store_idempotency`, `get_idempotency`, `cleanup_expired_idempotency`
  - [x] 3.1.3 MemoryStorage: `DashMap<String, (Vec<u8>, DateTime<Utc>)>` for idempotency entries
  - [x] 3.1.4 RocksDB: new `idempotency` column family (8th CF), key = idempotency_key, value = JSON(response + expiry)

- [x] 3.2 Engine implementation
  - [x] 3.2.1 In `Engine::spawn()`: check/store idempotency, 24h expiry
  - [x] 3.2.2 Same pattern in `Engine::transition()` (both normal and OR_STAY paths)
  - [x] 3.2.3 Cleanup via `cleanup_expired_idempotency()` (storage method, can be called periodically)
  - [x] 3.2.4 Engine tests: 10 tests (5 engine + 3 parser + 2 storage)

- [x] 3.3 Parser additions
  - [x] 3.3.1 Parse `SPAWN Machine { ... } IDEMPOTENCY_KEY "key"` — IDEMPOTENCY_KEY keyword added
  - [x] 3.3.2 Parse `TRANSITION Machine "id" TO state IDEMPOTENCY_KEY "key"` (also TRY TRANSITION)
  - [x] 3.3.3 Parser tests included in test_idempotency.rs

- [x] 3.4 Server + SDK
  - [x] 3.4.1 Server: `Idempotency-Key` HTTP header extracted and injected into commands
  - [x] 3.4.2 SDK: `TransitionOptions.idempotency_key` field, sent as HTTP header
  - [x] 3.4.3 Both SMQL syntax and HTTP header approaches supported (header overridden by SMQL)

- [x] 3.5 CHECKPOINT: All existing tests pass + 10 new idempotency tests pass

---

### Enhancement 4: Auth-to-Actor Binding [STATUS: COMPLETE]

> **Why:** Currently, the JWT auth middleware gates API access but does NOT populate
> the actor field in SMQL commands. An agent authenticated as `agent-A` can claim
> to be `agent-B` in its SMQL `AS` clause. Guards on `ACTOR.role` trust whatever
> the caller passes. In multi-agent deployments, you need verifiable identity.

- [ ] 4.1 Server middleware changes (feature-gated under `auth`)
  - [ ] 4.1.1 After JWT validation, extract `sub` (subject) and `role` claims from the token
  - [ ] 4.1.2 Store extracted identity in axum request extensions: `AuthenticatedActor { id: String, role: String, claims: HashMap<String, String> }`
  - [ ] 4.1.3 In execute handler: if `AuthenticatedActor` is present in extensions, override `as_actor` on SpawnCommand/TransitionCommand/ClaimCommand with the JWT identity — ignore whatever the SMQL `AS` clause says
  - [ ] 4.1.4 Add config option `trust_client_actor: bool` (default: false) — when true, allow the SMQL `AS` clause to override JWT identity (for development/testing)
  - [ ] 4.1.5 Add `actor` field to trail entries sourced from JWT, not client-provided string

- [ ] 4.2 Actor model enrichment
  - [ ] 4.2.1 Add `ACTOR.capabilities` — a list of strings derived from JWT `permissions` or `scope` claim
  - [ ] 4.2.2 Support guard: `"transition" IN ACTOR.capabilities` — check if actor has specific permission
  - [ ] 4.2.3 Eval context: `ACTOR` now evaluates to `Map({ id, role, capabilities: List([...]) })`

- [ ] 4.3 Tests
  - [ ] 4.3.1 Auth tests: 4 tests
    - JWT sub/role override client-provided AS clause
    - trust_client_actor=true allows AS clause
    - ACTOR.capabilities guard evaluation
    - Trail entries record JWT identity
  - [ ] 4.3.2 Regression: non-auth mode still uses AS clause as before

- [ ] 4.4 CHECKPOINT: All existing tests pass + new tests pass

---

## Tier 2: High-Impact Enhancements

### Enhancement 5: AST-Based Recovery Option Generation [STATUS: COMPLETE]

> **Why:** Current `generate_recovery_options()` uses string pattern matching
> (`contains("IS SET")`, `contains("ACTOR")`). Complex guards like
> `total / count > threshold AND assignee IN {agent_pool}` produce only a generic
> "Escalate". Walking the AST instead would make recovery options precise and
> actionable for any guard expression.

- [ ] 5.1 Refactor `generate_recovery_options` in `smql-engine/src/engine.rs`
  - [ ] 5.1.1 Accept `&[Expression]` (guard ASTs) instead of `&[GuardFailure]` (strings)
  - [ ] 5.1.2 Recursive AST visitor that pattern-matches on expression nodes:
    - `IsSet(FieldAccess(field))` → `SetField { field, suggested_value: "Provide a value" }`
    - `IsNotSet(FieldAccess(field))` → `SetField { field: None, reason: "Field must be absent" }`
    - `BinaryOp(FieldAccess(f), Eq, Literal(v))` → `SetField { field: f, suggested_value: v.to_string() }`
    - `BinaryOp(FieldAccess(f), Gt/Gte, Literal(n))` → `SetField { field: f, suggested_value: "must be > n" }`
    - `BinaryOp(FieldAccess(f), Lt/Lte, Literal(n))` → `SetField { field: f, suggested_value: "must be < n" }`
    - `BinaryOp(ActorRef, Eq, FieldAccess(f))` → `ChangeActor { suggested_value: "set actor to value of field f" }`
    - `BinaryOp(QualifiedAccess(ACTOR, "role"), Eq, Literal(v))` → `ChangeActor { suggested_value: v }`
    - `InSet { expr: ActorRef, values }` → `ChangeActor { suggested_value: "must be one of [values]" }`
    - `InSet { expr: FieldAccess(f), values }` → `SetField { field: f, suggested_value: "must be one of [values]" }`
    - `FunctionCall("elapsed"/"elapsed_in_state", _)` in comparison → `Wait { reason: "time condition" }`
    - `FunctionCall("timeout_remaining", _)` → `Wait`
    - `All/Any { collection, predicate }` → `Escalate { reason: "child condition: {predicate}" }`
    - `StateIs(s)` → `Retry { reason: "instance must be in state {s}" }`
    - `BinaryOp(left, And, right)` → recurse on both sides, merge results
    - `BinaryOp(left, Or, right)` → recurse on both sides, mark as "any of these"
    - Fallback: `Escalate { reason: "Complex guard: {expr}" }`
  - [ ] 5.1.3 Include `example` field with generated SMQL command for each recovery option
  - [ ] 5.1.4 Store the guard `Expression` AST nodes alongside `TransitionDefinition` (they are already there in `guards: Vec<Expression>`) — ensure they are passed through to recovery generation

- [ ] 5.2 Store guard AST in `TransitionDeniedError`
  - [ ] 5.2.1 Add `guard_expressions: Vec<Expression>` field to `TransitionDeniedError` (or pass through at generation time)
  - [ ] 5.2.2 `generate_recovery_options` now receives the original AST, not just failure strings

- [ ] 5.3 Tests
  - [ ] 5.3.1 Unit tests on recovery generation: 8 tests
    - IS SET guard → SetField with field name
    - Comparison guard (field > N) → SetField with range hint
    - ACTOR.role guard → ChangeActor with expected role
    - Elapsed guard → Wait
    - Compound AND guard → multiple recovery options
    - Compound OR guard → "any of" recovery options
    - ALL/ANY child guard → Escalate with child context
    - Complex nested expression → graceful fallback
  - [ ] 5.3.2 Integration test: transition denied response has precise recovery options for complex guard

- [ ] 5.4 CHECKPOINT: All existing tests pass + new tests pass

---

### Enhancement 6: Durable Event Log with Replay [STATUS: COMPLETE]

> **Why:** The current EventBus is `tokio::broadcast` with capacity 256. If a subscriber
> lags or disconnects, events are silently dropped. Agents that crash and restart have
> no way to catch up on missed events — they must re-scan the entire state space.
> Durable events with replay enable reliable agent-to-agent coordination.

- [ ] 6.1 Event persistence
  - [ ] 6.1.1 Define `StoredEvent` struct: `id: String (ULID)`, `timestamp: DateTime<Utc>`, `machine: String`, `event_name: String`, `instance_id: String`, `payload: serde_json::Value`, `actor: Option<String>`
  - [ ] 6.1.2 Add to Storage trait:
    ```
    async fn store_event(event: &StoredEvent) -> SmqlResult<()>
    async fn get_events_after(after_id: Option<&str>, machine: Option<&str>, event: Option<&str>, limit: usize) -> SmqlResult<Vec<StoredEvent>>
    async fn cleanup_events_before(before: DateTime<Utc>) -> SmqlResult<usize>
    ```
  - [ ] 6.1.3 MemoryStorage: `Vec<StoredEvent>` (append-only, sorted by ULID), binary search for after_id
  - [ ] 6.1.4 RocksDB: new `events` column family, key = ULID, value = serialized StoredEvent
    - Range scan from after_id to upper bound for efficient replay

- [ ] 6.2 Engine integration
  - [ ] 6.2.1 In `HookExecutor::execute_emit()`: after broadcasting on EventBus, also call `storage.store_event()`
  - [ ] 6.2.2 Auto-generate events for state transitions: `StoredEvent { event_name: "transition", instance_id, payload: { from_state, to_state, actor } }`
  - [ ] 6.2.3 Auto-generate events for spawns: `StoredEvent { event_name: "spawn", instance_id, payload: { machine, initial_state } }`
  - [ ] 6.2.4 Configurable: `event_retention: Duration` (default: 7 days) — cleanup runs on timer loop

- [ ] 6.3 Query support
  - [ ] 6.3.1 Parse `GET EVENTS Machine AFTER "event_id" LIMIT n`
  - [ ] 6.3.2 Parse `GET EVENTS AFTER "event_id" LIMIT n` (all machines)
  - [ ] 6.3.3 Add `EventsResult` to QueryResult
  - [ ] 6.3.4 Parser tests: 3 tests

- [ ] 6.4 WebSocket replay
  - [ ] 6.4.1 `/subscribe` endpoint accepts `?after=<event_id>` query param
  - [ ] 6.4.2 On connect: replay all stored events after the given ID, then switch to live streaming
  - [ ] 6.4.3 Each WebSocket message includes the event's ULID so the client can track its position
  - [ ] 6.4.4 Client reconnect pattern: store last received event ID, reconnect with `?after=<last_id>`

- [ ] 6.5 SDK additions
  - [ ] 6.5.1 `SmqlClient::get_events(after: Option<&str>, machine: Option<&str>, limit: usize)` → `Vec<StoredEvent>`
  - [ ] 6.5.2 `Subscription::new_with_replay(after_event_id: &str)` — reconnects with replay
  - [ ] 6.5.3 `Subscription::last_event_id()` — returns ID of last received event

- [ ] 6.6 Tests
  - [ ] 6.6.1 Storage tests: 4 tests (store, get_after, get_after with machine filter, cleanup)
  - [ ] 6.6.2 Engine tests: 3 tests (transition generates event, spawn generates event, EMIT generates event)
  - [ ] 6.6.3 Server tests: 3 tests (GET EVENTS via execute, WebSocket replay, event ULID ordering)
  - [ ] 6.6.4 SDK tests: 2 tests (get_events, subscription replay)

- [ ] 6.7 CHECKPOINT: All existing tests pass + new tests pass

---

### Enhancement 7: Conditional Wait / Watch [STATUS: COMPLETE]

> **Why:** Agents currently poll: `loop { FIND WHERE condition; sleep(1s); }`.
> This wastes compute and adds latency. A watch/wait primitive lets agents block
> until a condition becomes true, eliminating polling entirely.

- [ ] 7.1 AST additions
  - [ ] 7.1.1 Add `WatchCommand` to `smql-ast/src/command.rs`:
    - `machine: String`
    - `instance_id: Option<String>` — watch specific instance, or any matching
    - `condition: Expression` — the UNTIL condition
    - `timeout: Option<SmqlDuration>` — max wait time
    - `filter: Option<Expression>` — WHERE clause (when instance_id is None)

- [ ] 7.2 Parser
  - [ ] 7.2.1 Parse `WATCH Machine "instance_id" UNTIL STATE IS resolved TIMEOUT 30s`
  - [ ] 7.2.2 Parse `WATCH Machine WHERE priority == "high" UNTIL STATE IS escalated TIMEOUT 1m`
  - [ ] 7.2.3 Parser tests: 3 tests (instance watch, filtered watch, timeout)

- [ ] 7.3 Engine implementation
  - [ ] 7.3.1 `Engine::watch(cmd: &WatchCommand) -> SmqlResult<WatchResult>`
    - Check condition immediately — if already true, return instantly
    - If not true, register a `Watcher` in a `WatcherRegistry`:
      - `WatcherRegistry: DashMap<machine, Vec<Watcher>>`
      - `Watcher { id: Uuid, condition: Expression, instance_id: Option<String>, filter: Option<Expression>, sender: oneshot::Sender<Instance> }`
    - After every transition/spawn, check all watchers for that machine:
      - If watcher's condition is now satisfied → send instance through the oneshot → remove watcher
    - Timeout: register a timer that cancels the watcher and returns `TimeoutError`
    - Return `WatchResult { instance: Instance, waited: Duration }`
  - [ ] 7.3.2 Add watcher check to `Engine::transition_inner()` — after successful transition, call `check_watchers(machine, instance)`
  - [ ] 7.3.3 Add watcher check to `Engine::spawn()` — after successful spawn
  - [ ] 7.3.4 `Engine::cancel_watch(watcher_id)` — remove a watcher before it fires

- [ ] 7.4 Server wiring
  - [ ] 7.4.1 WATCH via `POST /execute` — long-poll: hold the HTTP connection open until condition met or timeout
  - [ ] 7.4.2 Set a maximum server-side timeout (e.g., 5 minutes) to prevent indefinite connections
  - [ ] 7.4.3 Return `WatchResult` JSON with the matched instance and wait duration

- [ ] 7.5 SDK additions
  - [ ] 7.5.1 `SmqlClient::watch(machine, instance_id, condition, timeout)` → `SdkResult<WatchResult>`
  - [ ] 7.5.2 Uses reqwest with extended timeout matching the watch timeout

- [ ] 7.6 Tests
  - [ ] 7.6.1 Engine tests: 6 tests
    - Condition already true → returns immediately
    - Condition becomes true after transition → returns after wait
    - Timeout fires → returns TimeoutError
    - Watch with filter (WHERE clause) matches correct instance
    - Cancel watch before it fires
    - Multiple watchers on same machine — each fires independently
  - [ ] 7.6.2 Server tests: 2 tests (long-poll success, timeout returns 408)

- [ ] 7.7 CHECKPOINT: All existing tests pass + new tests pass

---

### Enhancement 8: FIND with Field Projection [STATUS: COMPLETE]

> **Why:** FIND returns all fields on every instance. Agents querying thousands of
> instances pay for serializing data they don't need. Field projection reduces
> payload size, serialization cost, and makes agent code cleaner.

- [ ] 8.1 AST additions
  - [ ] 8.1.1 Add `select: Option<Vec<String>>` field to `FindQuery` in `smql-ast/src/query.rs`
    - None = all fields (current behavior), Some = only listed fields + always include id/state/machine

- [ ] 8.2 Parser
  - [ ] 8.2.1 Parse `FIND Machine SELECT field1, field2, field3 WHERE expr`
  - [ ] 8.2.2 SELECT comes before WHERE in the grammar
  - [ ] 8.2.3 Parser tests: 2 tests (with select, without select unchanged)

- [ ] 8.3 Engine implementation
  - [ ] 8.3.1 In `execute_find()`: after filtering, if `select` is Some, strip instance data to only selected fields (plus id, state, machine, created_at, updated_at — always included)
  - [ ] 8.3.2 Engine tests: 3 tests (select specific fields, select with WHERE, no select = all fields)

- [ ] 8.4 Server + SDK
  - [ ] 8.4.1 JSON response respects projection — excluded fields are not serialized
  - [ ] 8.4.2 SDK `FindBuilder::select(fields: &[&str])` method
  - [ ] 8.4.3 Server test: 1 test, SDK test: 1 test

- [ ] 8.5 CHECKPOINT: All existing tests pass + new tests pass

---

### Enhancement 9: Instance Tags / Metadata [STATUS: COMPLETE]

> **Why:** Agents need operational metadata (batch ID, source agent, retry count,
> priority override, correlation ID) without polluting the machine's typed DATA schema.
> Tags are untyped, indexed, and not subject to schema validation.

- [ ] 9.1 Data model
  - [ ] 9.1.1 Add `tags: HashMap<String, String>` to `Instance` struct
  - [ ] 9.1.2 Default to empty HashMap (no tags)

- [ ] 9.2 Storage indexing
  - [ ] 9.2.1 MemoryStorage: `DashMap<(String, String), HashSet<InstanceId>>` for tag index (tag_key, tag_value) → instance IDs
  - [ ] 9.2.2 RocksDB: new `tag_index` column family, key = `{tag_key}\0{tag_value}\0{instance_id}`
  - [ ] 9.2.3 Update tag index on spawn, transition (if tags change), and delete
  - [ ] 9.2.4 Storage trait: `find_by_tag(key: &str, value: &str) -> SmqlResult<Vec<InstanceId>>`

- [ ] 9.3 SMQL syntax
  - [ ] 9.3.1 Parse `SPAWN Machine { ... } TAGS { batch: "run-42", agent: "planner" }`
  - [ ] 9.3.2 Parse `TRANSITION Machine "id" TO state TAGS { retry_count: "3" }`
  - [ ] 9.3.3 Parse `FIND Machine WHERE TAG "batch" == "run-42"`
  - [ ] 9.3.4 Add `tags: Option<HashMap<String, String>>` to SpawnCommand and TransitionCommand
  - [ ] 9.3.5 Parser tests: 3 tests

- [ ] 9.4 Engine implementation
  - [ ] 9.4.1 On spawn: if tags provided, set on instance before storing
  - [ ] 9.4.2 On transition: if tags provided, merge into existing tags (overwrite matching keys)
  - [ ] 9.4.3 TAG filter in FIND WHERE: evaluate `TAG "key" == "value"` against instance.tags
  - [ ] 9.4.4 Engine tests: 5 tests (spawn with tags, transition updates tags, FIND by tag, tag merge, tag in JSON response)

- [ ] 9.5 Server + SDK
  - [ ] 9.5.1 Tags included in instance JSON responses
  - [ ] 9.5.2 SDK: `SpawnOptions.tag(key, value)`, `TransitionOptions.tag(key, value)`
  - [ ] 9.5.3 SDK: `FindBuilder.with_tag(key, value)`
  - [ ] 9.5.4 Tests: 2 server, 2 SDK

- [ ] 9.6 CHECKPOINT: All existing tests pass + new tests pass

---

### Enhancement 10: FIND Missing Query Predicates [STATUS: COMPLETE]

> **Why:** Several query predicates are lexed as keywords but never wired into the
> query engine. Agents need these for common introspection patterns:
> "which instances are stuck?", "which ones have ever visited state X?",
> "which are alive vs terminated?"

- [ ] 10.1 ALIVE / TERMINATED predicates
  - [ ] 10.1.1 Parse `FIND Machine WHERE ALIVE` → filter: current state NOT in terminal_states
  - [ ] 10.1.2 Parse `FIND Machine WHERE TERMINATED` → filter: current state IN terminal_states
  - [ ] 10.1.3 Engine: look up machine's terminal_states, compare instance.state
  - [ ] 10.1.4 Tests: 2 tests

- [ ] 10.2 STUCK_IN predicate
  - [ ] 10.2.1 Parse `FIND Machine WHERE STUCK_IN(state, > 24h)` into query filter
  - [ ] 10.2.2 Engine: check `instance.state == state AND elapsed_in_state() > duration`
  - [ ] 10.2.3 Tests: 2 tests (stuck, not stuck)

- [ ] 10.3 HAS_VISITED / NEVER_VISITED predicates
  - [ ] 10.3.1 Parse `FIND Machine WHERE HAS_VISITED(state_name)` and `NEVER_VISITED(state_name)`
  - [ ] 10.3.2 Engine: for each candidate instance, load trail and check if `to_state` includes the target
    - Performance note: this requires trail scans — add a warning in docs for large datasets
  - [ ] 10.3.3 Tests: 2 tests (visited, never visited)

- [ ] 10.4 TRAIL time filters
  - [ ] 10.4.1 Parse `TRAIL OF "id" SINCE "2024-01-01"` and `TRAIL OF "id" UNTIL "2024-12-31"` into TrailFilter.after / TrailFilter.before
  - [ ] 10.4.2 Engine: filter trail entries by timestamp range
  - [ ] 10.4.3 Tests: 2 tests

- [ ] 10.5 CHECKPOINT: All existing tests pass + new tests pass

---

## Tier 3: Nice-to-Have Features

### Enhancement 11: Atomic Multi-Instance Transactions [STATUS: COMPLETE]

> **Why:** Agents orchestrating multi-step workflows need atomicity. Currently,
> CASCADE/SIGNAL PARENT/saga steps each write independently. If step 3 of 5 fails,
> steps 1-2 are committed. Partial completion leaves inconsistent state.

- [ ] 11.1 Transaction API
  - [ ] 11.1.1 Parse `BEGIN ... COMMIT` block containing multiple SPAWN/TRANSITION statements
  - [ ] 11.1.2 `Engine::execute_transaction(statements: Vec<Statement>) -> SmqlResult<Vec<StatementResult>>`
    - Collect all writes in a buffer
    - Validate all operations first (dry-run guard evaluation)
    - Apply all writes atomically (MemoryStorage: hold all shard locks; RocksDB: single WriteBatch)
    - On any failure: rollback all buffered writes
  - [ ] 11.1.3 Return all individual results on success, or the first error on failure

- [ ] 11.2 Storage layer
  - [ ] 11.2.1 Add `begin_transaction() -> TransactionHandle` and `commit(handle)` / `rollback(handle)` to Storage trait
  - [ ] 11.2.2 MemoryStorage: buffer writes in TransactionHandle, apply on commit
  - [ ] 11.2.3 RocksDB: use TransactionDB with multi-key get_for_update

- [ ] 11.3 Saga executor (complete existing stub)
  - [ ] 11.3.1 Implement `Engine::execute_saga()` with step-by-step execution
  - [ ] 11.3.2 On step failure: execute compensation steps in reverse order
  - [ ] 11.3.3 Emit `SagaCompleted` / `SagaCompensated` events on EventBus
  - [ ] 11.3.4 Surface saga step failures to the caller (not just warn-level logs)

- [ ] 11.4 Tests
  - [ ] 11.4.1 Transaction: 4 tests (commit success, rollback on failure, concurrent transaction conflict, nested not allowed)
  - [ ] 11.4.2 Saga: 4 tests (happy path, compensation on failure, compensation step fails, event emission)

- [ ] 11.5 CHECKPOINT: All existing tests pass + new tests pass

---

### Enhancement 12: Computed Fields [STATUS: COMPLETE]

> **Why:** COMPUTED constraint is parsed but never evaluated. Agents need derived
> values (elapsed time, percentages, counts) without manually computing them
> client-side on every query.

- [ ] 12.1 Engine implementation
  - [ ] 12.1.1 In `Instance` read path (get_instance, find_instances): for each COMPUTED field in the machine definition, evaluate the expression against the instance's EvalContext and inject the result into the returned data
  - [ ] 12.1.2 COMPUTED fields are read-only — reject WITH mutations that target a COMPUTED field
  - [ ] 12.1.3 COMPUTED fields are not stored — they are evaluated on every read

- [ ] 12.2 Common computed expressions
  - [ ] 12.2.1 `elapsed_in_state()` — already implemented as a function, now also usable as a field default
  - [ ] 12.2.2 `count(child_collection)` — number of children
  - [ ] 12.2.3 Arithmetic over data fields: `subtotal * quantity`

- [ ] 12.3 Tests
  - [ ] 12.3.1 3 tests: computed field in GET, computed field in FIND, reject mutation of computed field

- [ ] 12.4 CHECKPOINT: All existing tests pass + new tests pass

---

### Enhancement 13: Bulk Spawn [STATUS: COMPLETE]

> **Why:** Agents bootstrapping workflows often need to create hundreds of instances.
> Sequential HTTP calls are slow. A batch spawn reduces round trips and uses a
> single storage write batch.

- [ ] 13.1 AST + Parser
  - [ ] 13.1.1 Parse `SPAWN BATCH Machine [{ field: value }, { field: value }, ...]`
  - [ ] 13.1.2 Add `BatchSpawnCommand` to AST: `machine: String, instances: Vec<HashMap<String, Value>>`

- [ ] 13.2 Engine
  - [ ] 13.2.1 `Engine::batch_spawn(cmd: &BatchSpawnCommand) -> SmqlResult<BatchSpawnResult>`
    - Validate all instances first (required fields, types, constraints)
    - Generate all ULIDs
    - Store all instances in a single storage batch write
    - Return list of instance IDs and any per-instance failures
  - [ ] 13.2.2 `BatchSpawnResult { created: Vec<String>, failures: Vec<BatchSpawnFailure> }`

- [ ] 13.3 Tests
  - [ ] 13.3.1 4 tests: batch spawn success, partial failure (some invalid), empty batch, batch with tags

- [ ] 13.4 CHECKPOINT: All existing tests pass + new tests pass

---

### Enhancement 14: Webhook Response Handling [STATUS: COMPLETE]

> **Why:** Current webhooks are fire-and-forget. Agents need webhooks that feed
> responses back into the state machine — e.g., call an LLM API, use the response
> to set a field and auto-transition. This turns SMQL into an active orchestrator,
> not just a passive state store.

- [ ] 14.1 AST additions
  - [ ] 14.1.1 Extend `WebhookAction` with response handling:
    - `on_success: Option<Vec<MutateClause>>` — mutations to apply with RESPONSE as a context variable
    - `on_failure: Option<String>` — state to transition to on webhook failure
    - `response_field: Option<String>` — field to store the full response body

- [ ] 14.2 Engine implementation
  - [ ] 14.2.1 `WebhookClient::execute_with_response()` — returns parsed JSON response body
  - [ ] 14.2.2 After successful webhook: inject `RESPONSE` into EvalContext, evaluate on_success mutations, apply to instance
  - [ ] 14.2.3 After failed webhook: if `on_failure` state is set, transition instance to that state
  - [ ] 14.2.4 Webhook response timeout: configurable per-webhook (default: 30s)

- [ ] 14.3 Tests
  - [ ] 14.3.1 4 tests: response stored in field, on_success mutation applied, on_failure transition, timeout handling
  - [ ] 14.3.2 Note: tests use mock HTTP server (wiremock or similar)

- [ ] 14.4 CHECKPOINT: All existing tests pass + new tests pass

---

### Enhancement 15: Instance TTL / Auto-Expiry [STATUS: COMPLETE]

> **Why:** Agents create ephemeral instances (scratch state, coordination tokens,
> temporary tasks). These should auto-expire without manual cleanup, preventing
> unbounded state growth.

- [ ] 15.1 Data model
  - [ ] 15.1.1 Add `expires_at: Option<DateTime<Utc>>` to `Instance`
  - [ ] 15.1.2 Parse `SPAWN Machine { ... } TTL 1h` — sets expires_at = now + duration
  - [ ] 15.1.3 Parse `SPAWN Machine { ... } EXPIRES_AT "2024-12-31T00:00:00Z"`

- [ ] 15.2 Engine
  - [ ] 15.2.1 Register TTL expiry in TimerManager (key: `ttl:{instance_id}`)
  - [ ] 15.2.2 On expiry: delete instance (including children, timers, claims)
  - [ ] 15.2.3 FIND queries exclude expired instances by default
  - [ ] 15.2.4 GET on expired instance returns 410 Gone (not 404)

- [ ] 15.3 Tests
  - [ ] 15.3.1 3 tests: TTL spawn, auto-expiry deletes instance, FIND excludes expired

- [ ] 15.4 CHECKPOINT: All existing tests pass + new tests pass

---

### Enhancement 16: Machine Templates / Inheritance [STATUS: COMPLETE]

> **Why:** Agents managing similar workflows (ticket-agent, order-agent, review-agent)
> share common patterns. Templates reduce boilerplate and enforce consistency.

- [ ] 16.1 AST + Parser
  - [ ] 16.1.1 Parse `DEFINE TEMPLATE name { ... }` — same structure as DEFINE MACHINE but no instances
  - [ ] 16.1.2 Parse `DEFINE MACHINE MyMachine EXTENDS template_name { ... }` — inherits states, transitions, data from template, can override/add

- [ ] 16.2 Catalog
  - [ ] 16.2.1 Store templates in catalog separate from machines
  - [ ] 16.2.2 On `DEFINE MACHINE ... EXTENDS`: merge template definition with machine-specific additions
  - [ ] 16.2.3 Conflict resolution: machine-specific definitions override template

- [ ] 16.3 Tests
  - [ ] 16.3.1 4 tests: define template, extend template, override template state, extend with additional states

- [ ] 16.4 CHECKPOINT: All existing tests pass + new tests pass

---

## Cross-Cutting Concerns

### CC-1: SDK Error Surface Improvement [STATUS: COMPLETE]

> **Why:** The SDK currently serializes `TransitionDeniedError` to a flat string,
> losing the structured `recovery_options` and `llm_prompt` fields. Agents using
> the SDK must parse raw JSON to get actionable error data.

- [x] CC-1.1 Return structured `TransitionDeniedError` in SDK error types (not just a string message)
- [x] CC-1.2 `SdkError::TransitionDenied` should carry `recovery_options: Vec<RecoveryOption>` and `llm_prompt: Option<String>`
- [x] CC-1.3 SDK `TransitionOptions` should support `role` field (not just actor ID)
- [x] CC-1.4 Tests: 3 tests (structured error with accessors, server retryable flag, no-detail accessors)

### CC-2: EventBus Capacity Configuration [STATUS: COMPLETE]

> **Why:** Fixed capacity of 256 causes event loss under load. Should be configurable.

- [ ] CC-2.1 Make EventBus capacity configurable via `EngineConfig` or server config
- [ ] CC-2.2 Default: 1024 (up from 256)
- [ ] CC-2.3 Emit a metric `smql_events_dropped_total` when subscribers lag
- [ ] CC-2.4 Tests: 1 test (configurable capacity)

### CC-3: HTTP Error Surface [STATUS: COMPLETE]

> **Why:** Internal and storage errors are scrubbed to "Internal server error" in
> HTTP responses. Agents over HTTP can't distinguish retryable storage conflicts
> from permanent errors.

- [ ] CC-3.1 Include `retryable: bool` in 500 error JSON responses
- [ ] CC-3.2 Include error category (storage, internal, timeout) in response
- [ ] CC-3.3 Never expose internal error details (security) but do expose retryability
- [ ] CC-3.4 Tests: 2 tests (retryable error includes hint, non-retryable doesn't)

---

## Implementation Order Summary

| Order | Enhancement | Est. Tests | Depends On |
|-------|-----------|-----------|-----------|
| 1 | E1: EXPLAIN TRANSITIONS | ~15 | COMPLETE |
| 2 | E5: AST-Based Recovery | ~10 | COMPLETE |
| 3 | CC-1: SDK Error Surface | ~4 | COMPLETE |
| 4 | CC-3: HTTP Error Surface | ~4 | COMPLETE |
| 5 | E3: Idempotency Keys | ~12 | COMPLETE |
| 6 | E4: Auth-to-Actor Binding | ~5 | COMPLETE |
| 7 | E8: FIND Projection | ~7 | COMPLETE |
| 8 | E10: Missing Query Predicates | ~8 | COMPLETE |
| 9 | E9: Instance Tags | ~12 | COMPLETE |
| 10 | E2: Instance Claiming | 15 | COMPLETE |
| 11 | E6: Durable Event Log | 10 | COMPLETE |
| 12 | E7: Conditional Wait | ~11 | COMPLETE (E6 done) |
| 13 | CC-2: EventBus Capacity | 3 | COMPLETE |
| 14 | E12: Computed Fields | 5 | COMPLETE |
| 15 | E13: Bulk Spawn | 6 | COMPLETE |
| 16 | E15: Instance TTL | 4 | COMPLETE |
| 17 | E11: Transactions + Sagas | ~8 | — |
| 18 | E14: Webhook Responses | 14 | COMPLETE |
| 19 | E16: Machine Templates | 6 | COMPLETE |

**Estimated total: ~153 new tests across all enhancements**

---

## Notes for Future Sessions

- Read this file first to understand the full scope
- Each enhancement is self-contained — can be implemented independently unless "Depends On" is noted
- Always run the full test suite after each enhancement: `cargo test --workspace`
- For RocksDB-specific changes: `cargo test --workspace --features rocksdb`
- For auth-specific changes: `cargo test --workspace --features auth`
- Update the main `CHECKLIST.md` as enhancements are completed
- Update `MEMORY.md` with new design decisions and pitfalls discovered during implementation
