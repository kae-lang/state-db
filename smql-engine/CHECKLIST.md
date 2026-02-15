# SMQL Engine — Build Checklist

> Last updated: 2026-02-15
> Current phase: Phase 8
> Current agent focus: Engine-Dev

---

## Phase 1: Foundation & Data Model [STATUS: COMPLETE]

- [x] 1.1 Initialize Cargo workspace with all crates (empty libs) — 2026-02-15
- [x] 1.2 Define core types in smql-ast — 2026-02-15
  - [x] 1.2.1 MachineDefinition struct (name, states, initial, terminals)
  - [x] 1.2.2 StateDefinition struct (name, metadata)
  - [x] 1.2.3 TransitionDefinition struct (from, to, guards, actions, timeout, mutates)
  - [x] 1.2.4 DataFieldDefinition struct (name, type, constraints)
  - [x] 1.2.5 Expression enum (for guards — binary ops, field access, literals, function calls)
  - [x] 1.2.6 Action enum (Notify, Log, Emit, Webhook, SpawnChild)
  - [x] 1.2.7 Query AST nodes (Find, Get, Aggregate, Trail, Paths, Funnel)
  - [x] 1.2.8 Command AST nodes (Spawn, Transition, TryTransition, BatchTransition, AlterMachine)
  - [x] 1.2.9 Value enum (Text, Int, Float, Bool, Uuid, DateTime, Duration, List, Map, Null, Ref)
  - [x] 1.2.10 TypeDefinition enum (all SMQL types: Text, Int, Float, Bool, Uuid, Date, DateTime, Duration, Enum, Ref, List, Set, Map, Blob, Money, Json)
- [x] 1.3 Set up error types crate-wide (thiserror) — 2026-02-15
  - [x] 1.3.1 SmqlError enum with variants: ParseError, ValidationError, TransitionDenied, GuardFailed, SpawnRejected, QueryError, StorageError, TimeoutError
  - [x] 1.3.2 TransitionDeniedError with structured guard failure details (guard_expr, actual_value, hint)
- [x] 1.4 Write unit tests for all core types (ser/de, Display, Clone, PartialEq) — 71 tests passing
- [x] 1.5 CHECKPOINT: `cargo build` succeeds, all tests pass — 2026-02-15

## Phase 2: SMQL Parser [STATUS: COMPLETE]

- [x] 2.1 Hand-written tokenizer (no winnow dependency for parsing itself — used as workspace dep only) — 2026-02-15
- [x] 2.2 Lexer/tokenizer — 62 tests passing
  - [x] 2.2.1-2.2.5 Keywords, identifiers, literals, operators, punctuation, comments
- [x] 2.3 DEFINE MACHINE parser — parses support_ticket.smql and order.smql
  - [x] 2.3.1-2.3.8 DATA, STATES, INITIAL, TERMINAL, TRANSITIONS, CHILDREN, HOOKS, ROLES, wildcards
- [x] 2.4 Command parsers (SPAWN, TRANSITION, TRY, ALTER MACHINE)
- [x] 2.5 Query parsers (GET, FIND, AGGREGATE, TRAIL, PATHS, FUNNEL, COMPARE PATHS)
- [x] 2.6 Expression parser (arithmetic, comparison, logical, field access, functions, state predicates, null checks, set membership, patterns)
- [x] 2.7 Span-based error messages with hints
- [x] 2.8 62 parser tests (lexer, expressions, machines, commands, queries)
- [x] 2.9 Both example .smql files parse end-to-end
- [x] 2.10 CHECKPOINT: All 62 tests pass — 2026-02-15

## Phase 3: Catalog & Validation [STATUS: COMPLETE]

- [x] 3.1 Implement MachineCatalog (in-memory registry of machine definitions) — 2026-02-15
  - [x] 3.1.1 Register/unregister machines
  - [x] 3.1.2 Retrieve machine by name
  - [x] 3.1.3 Validate machine definition on registration
- [x] 3.2 Machine validation rules — 22 tests passing
  - [x] 3.2.1 Initial state must be in STATES set
  - [x] 3.2.2 Terminal states must be in STATES set
  - [x] 3.2.3 All transition sources/targets must be in STATES set
  - [x] 3.2.4 No transitions FROM terminal states (warn)
  - [x] 3.2.5 All states must be reachable from initial state (warn on unreachable)
  - [x] 3.2.6 Detect dead-end states (non-terminal states with no outgoing transitions)
  - [ ] 3.2.7 Guard expressions type-check against DATA fields (deferred to Phase 5)
  - [x] 3.2.8 REF targets must reference registered machines (warn)
  - [x] 3.2.9 CHILDREN machines must exist in catalog (warn)
  - [x] 3.2.10 Timeout target states must be valid
- [x] 3.3 Schema versioning (machine_name:version)
  - [x] 3.3.1 Auto-increment version on update
  - [x] 3.3.2 Store version history
  - [ ] 3.3.3 Support `smql diff` between versions (deferred)
- [x] 3.4 Catalog persistence (serialize/deserialize JSON)
- [x] 3.5 CHECKPOINT: Machines can be defined, validated, stored, and retrieved — 2026-02-15

## Phase 4: Storage Layer [STATUS: COMPLETE (MemoryStorage)]

- [x] 4.1 Define Storage trait (pluggable backend interface) — 2026-02-15
- [x] 4.2 Implement MemoryStorage (DashMap-based, for development and tests) — 27 tests passing
  - [x] 4.2.1 Instance storage with concurrent access (DashMap)
  - [x] 4.2.2 State index (state -> set of instance IDs) for fast state queries
  - [x] 4.2.3 Trail storage (append-only Vec per instance with RwLock)
  - [x] 4.2.4 Full-scan filtering with predicate pushdown (FilterPredicate)
- [ ] 4.3 Implement RocksDB storage backend (deferred to later phase)
  - [ ] 4.3.1 Key schema design
  - [ ] 4.3.2 Column families: instances, state_index, trails, catalog, timers
  - [ ] 4.3.3 Atomic transitions: WriteBatch for (update instance + update state index + append trail)
  - [ ] 4.3.4 Prefix iteration for efficient queries
  - [ ] 4.3.5 Compaction and TTL for old trail entries
- [x] 4.4 Instance data model (Instance, InstanceId, TrailEntry, Filter, Mutation) — 2026-02-15
- [x] 4.5 Write storage integration tests — 27 tests passing
- [x] 4.6 CHECKPOINT: Can store/retrieve/query instances with MemoryStorage — 2026-02-15

## Phase 5: Core Engine — Spawn & Transition [STATUS: COMPLETE]

- [x] 5.1 Implement Engine struct — 2026-02-15
- [x] 5.2 Implement SPAWN (data validation, defaults, trail entry, THEN TRANSITION)
- [x] 5.3 Implement TRANSITION (guards, mutations, wildcard, THROUGH, OR STAY, TRY)
- [x] 5.4 Guard expression evaluator — full AST walker with 24 eval tests
- [x] 5.5 Write comprehensive transition tests — 46 tests passing
- [x] 5.6 CHECKPOINT: Full spawn and transition lifecycle works — 2026-02-15

## Phase 6: Timer & Timeout System [STATUS: COMPLETE]

- [x] 6.1 Implement TimerManager — 2026-02-15
  - [x] 6.1.1 Timer registration: (instance_id, state, deadline, target_state)
  - [x] 6.1.2 Timer cancellation: cancel by (instance_id, state) when instance leaves state
  - [ ] 6.1.3 Timer storage: persist to storage backend (deferred — requires RocksDB)
  - [x] 6.1.4 BTreeMap priority queue for efficient "what fires next?" lookups
- [x] 6.2 Background timer thread/task — 2026-02-15
  - [x] 6.2.1 Tokio interval that checks for expired timers (configurable check interval)
  - [x] 6.2.2 On expiry: perform guard-free TRANSITION as System actor
  - [x] 6.2.3 Handle race condition: instance already transitioned (returns None)
  - [x] 6.2.4 Error handling for failed timeout transitions (silently ignored)
- [ ] 6.3 DWELL hooks (deferred to Phase 8 — requires hook infrastructure)
- [x] 6.4 TIMEOUT_REMAINING query function — 2026-02-15
  - [x] 6.4.1 Calculate remaining time from timer registry
  - [x] 6.4.2 Return Null for instances without active timeout
- [x] 6.5 Write timer tests — 26 tests (14 timer crate + 12 engine integration)
- [x] 6.6 CHECKPOINT: Timeouts work correctly — 266 total tests passing

## Phase 7: Query Engine [STATUS: COMPLETE]

- [x] 7.1 Query executor (in smql-engine-core) — 2026-02-15
- [x] 7.2 Implement FIND queries (WHERE filter, SORT, LIMIT, OFFSET)
- [x] 7.3 Implement GET queries (single instance by ID)
- [x] 7.4 Implement TRAIL queries (with actor/state filters)
- [x] 7.5 Implement temporal query functions (elapsed, NOW in evaluator)
- [x] 7.6 Implement AGGREGATE queries (COUNT, SUM, AVG, MIN, MAX, PERCENTILE, GROUP BY state/field)
- [x] 7.7 Implement PATHS query (state sequence analysis from trails)
- [x] 7.8 Implement FUNNEL query (conversion analysis through ordered states)
- [ ] 7.9 Query result formatting (deferred to server/CLI phases)
- [x] 7.10 Write query tests — 12 query tests passing
- [x] 7.11 CHECKPOINT: All query types work against test data — 2026-02-15

## Phase 8: Hooks & Actions [STATUS: COMPLETE]

- [x] 8.1 Implement smql-hooks crate — 2026-02-15
  - [x] 8.1.1 HookError (Rejected, ActionFailed, WebhookFailed)
  - [x] 8.1.2 HookContext (instance_id, machine, from/to state, data, actor, memo)
  - [x] 8.1.3 ResolvedAction enum (Notify, Log, Emit, Webhook, SpawnChild, SignalParent)
  - [x] 8.1.4 EngineCallback trait (spawn_child, signal_parent)
  - [x] 8.1.5 EventBus (tokio::broadcast channel + subscribe)
  - [x] 8.1.6 HookExecutor (trigger matching, action dispatch, BEFORE sync/reject)
  - [x] 8.1.7 Log template rendering ({instance_id}, {from_state}, {field} etc.)
- [x] 8.2 Engine integration — 2026-02-15
  - [x] 8.2.1 Engine.hook_executor: Arc<HookExecutor>
  - [x] 8.2.2 resolve_actions() / resolve_hooks_actions() — eval_expr on Action → ResolvedAction
  - [x] 8.2.3 Spawn: ON SPAWN + ON ENTER(initial_state) hooks fire
  - [x] 8.2.4 Transition: BEFORE → guards → mutations → write → ON EXIT → timers → ACTIONs → ON ENTER → AFTER
  - [x] 8.2.5 Timeout transition: ON EXIT + ON ENTER + AFTER EACH hooks fire
  - [x] 8.2.6 Engine.with_hooks() constructor, Engine.event_bus() accessor
- [x] 8.3 Webhook/Notify dry-run (log-only, no reqwest) — 2026-02-15
- [x] 8.4 SignalParent: logs warning (deferred to Phase 9)
- [x] 8.5 Write hook tests — 28 tests (14 smql-hooks unit + 14 engine integration)
- [x] 8.6 CHECKPOINT: All tests pass — 294 total tests passing — 2026-02-15
- [ ] 8.7 DWELL hooks (deferred — requires timer integration for dwell timers)

## Phase 9: Machine Composition [STATUS: NOT STARTED]

- [ ] 9.1-9.5 Parent-child relationships and checkpoint

## Phase 10: Server & Wire Protocol [STATUS: COMPLETE]

- [x] 10.1 HTTP/JSON server with axum — 2026-02-15
- [x] 10.2 POST /execute endpoint (parse + execute SMQL)
- [x] 10.3 GET /machines, GET /machines/:name
- [x] 10.4 GET /instances/:id
- [x] 10.5 GET /health
- [x] 10.6 JSON response formatting for all query types
- [x] 10.7 Error responses with appropriate HTTP status codes

## Phase 11: CLI & REPL [STATUS: COMPLETE]

- [x] 11.1 CLI binary with clap (serve, repl, exec, run subcommands) — 2026-02-15
- [x] 11.2 Interactive REPL with rustyline (multiline, history, dot-commands)
- [x] 11.3 File execution (smql run file.smql)
- [x] 11.4 Pretty-print query results in terminal

## Phase 12: Observability [STATUS: NOT STARTED]

- [ ] 12.1-12.4 Observability and checkpoint

## Phase 13: Schema Evolution [STATUS: NOT STARTED]

- [ ] 13.1-13.4 ALTER MACHINE and checkpoint

## Phase 14: Integration Tests & Examples [STATUS: NOT STARTED]

- [ ] 14.1-14.5 Integration tests and checkpoint

## Phase 15: SDK & Developer Experience Polish [STATUS: NOT STARTED]

- [ ] 15.1-15.4 SDK and final checkpoint
