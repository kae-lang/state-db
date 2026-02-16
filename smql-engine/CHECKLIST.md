# SMQL Engine — Build Checklist

> Last updated: 2026-02-16
> Current phase: Phase 15 COMPLETE + Bugfixes
> Current agent focus: Done

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

## Phase 4: Storage Layer [STATUS: COMPLETE]

- [x] 4.1 Define Storage trait (pluggable backend interface) — 2026-02-15
- [x] 4.2 Implement MemoryStorage (DashMap-based, for development and tests) — 42 tests passing
  - [x] 4.2.1 Instance storage with concurrent access (DashMap)
  - [x] 4.2.2 State index (state -> set of instance IDs) for fast state queries
  - [x] 4.2.3 Trail storage (append-only Vec per instance with RwLock)
  - [x] 4.2.4 Full-scan filtering with predicate pushdown (FilterPredicate)
- [x] 4.3 Implement RocksDB storage backend — 44 tests passing — 2026-02-15
  - [x] 4.3.1 Key schema design (6 column families with NUL-separated composite keys)
  - [x] 4.3.2 Column families: instances, state_index, machine_index, trails, parent_index, id_index
  - [x] 4.3.3 Atomic transitions: WriteBatch for (update instance + update state index + append trail)
  - [x] 4.3.4 Range iteration with upper bounds for efficient prefix queries
  - [ ] 4.3.5 Compaction and TTL for old trail entries (deferred)
  - [x] 4.3.6 Feature-gated: `cargo build --features rocksdb`
  - [x] 4.3.7 CLI --storage flag for selecting backend
  - [x] 4.3.8 Server SmqlServer::with_storage() constructor
  - [x] 4.3.9 Data persists across process restarts
- [x] 4.4 Instance data model (Instance, InstanceId, TrailEntry, Filter, Mutation) — 2026-02-15
- [x] 4.5 Write storage integration tests — 86 tests passing (42 memory + 44 rocksdb)
- [x] 4.6 CHECKPOINT: Can store/retrieve/query instances with MemoryStorage and RocksDB — 2026-02-15

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

## Phase 9: Machine Composition [STATUS: COMPLETE]

- [x] 9.1 Storage layer — parent-child tracking — 2026-02-15
  - [x] 9.1.1 Instance: parent_id, parent_machine fields + new_child() constructor
  - [x] 9.1.2 Storage trait: find_children, get_parent methods
  - [x] 9.1.3 MemoryStorage: parent_index (DashMap<String, HashSet<String>>), store/delete updates
  - [x] 9.1.4 7 parent-child storage tests
- [x] 9.2 SpawnCommand — parent context — 2026-02-15
  - [x] 9.2.1 SpawnCommand: parent_id, parent_machine optional fields
  - [x] 9.2.2 TransitionCommand: cascade bool field
  - [x] 9.2.3 Parser: CASCADE keyword in transition parsing
  - [x] 9.2.4 Engine spawn(): validate parent exists, create Instance::new_child
- [x] 9.3 EvalContext — children & parent access — 2026-02-15
  - [x] 9.3.1 ChildInfo struct, children/parent_data/parent_state in EvalContext
  - [x] 9.3.2 ALL/ANY quantifier evaluation over child collections
  - [x] 9.3.3 SignalFrom evaluation (child machine + condition matching)
  - [x] 9.3.4 FieldAccess: child.STATE, child.count, PARENT.field, PARENT.STATE
- [x] 9.4 Engine — EngineCallback & composition wiring — 2026-02-15
  - [x] 9.4.1 EngineCallbackImpl (spawn_child, signal_parent)
  - [x] 9.4.2 HookExecutor callback: RwLock for post-construction setting
  - [x] 9.4.3 Engine::wire_callback(), Engine::populate_composition_context()
  - [x] 9.4.4 __spawn handling in MUTATE (async spawn, Value::Ref result)
  - [x] 9.4.5 CASCADE transitions (recursive child cascading to terminal states)
- [x] 9.5 Composition tests — 22 engine + 7 storage = 29 tests — 2026-02-15
- [x] 9.6 CHECKPOINT: All 323 tests pass (294 existing + 29 new) — 2026-02-15

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

## Phase 12: Observability [STATUS: COMPLETE]

- [x] 12.1 Structured tracing spans on engine operations — 2026-02-15
  - [x] 12.1.1 `tracing` dep added to smql-engine-core
  - [x] 12.1.2 `#[tracing::instrument]` on spawn, transition_inner, timeout_transition, execute_query
  - [x] 12.1.3 `tracing::info!` on spawn success, transition complete, timeout fire
  - [x] 12.1.4 `tracing::warn!` on guard failures
- [x] 12.2 Prometheus metrics — 2026-02-15
  - [x] 12.2.1 SmqlMetrics struct with Registry (smql-server/src/metrics.rs)
  - [x] 12.2.2 smql_instances_total (IntGaugeVec: machine, state)
  - [x] 12.2.3 smql_transitions_total (IntCounterVec: machine, from, to)
  - [x] 12.2.4 smql_transition_duration_seconds (HistogramVec: machine)
  - [x] 12.2.5 smql_guard_failures_total (IntCounterVec: machine)
  - [x] 12.2.6 smql_timeout_fires_total (IntCounterVec: machine, state)
  - [x] 12.2.7 smql_query_duration_seconds (HistogramVec: query_type)
  - [x] 12.2.8 smql_spawns_total (IntCounterVec: machine)
  - [x] 12.2.9 GET /metrics endpoint (Prometheus text format)
  - [x] 12.2.10 Handler instrumentation (spawn, transition, query timing)
- [x] 12.3 WebSocket event streaming — 2026-02-15
  - [x] 12.3.1 GET /subscribe WebSocket endpoint (axum ws feature)
  - [x] 12.3.2 EventBus subscription with machine/event filtering
  - [x] 12.3.3 JSON event forwarding to WS clients
- [x] 12.4 JSON tracing output — 2026-02-15
  - [x] 12.4.1 tracing-subscriber JSON format with env filter in CLI serve
- [x] 12.5 Timeout metrics via EventBus listener — 2026-02-15
- [x] 12.6 Tests — 11 server tests (3 metrics unit + 5 metrics endpoint + 2 WebSocket + 1 health)
- [x] 12.7 CHECKPOINT: All 373 tests pass (362 existing + 11 new) — 2026-02-15

## Phase 13: Schema Evolution [STATUS: COMPLETE]

- [x] 13.1 ALTER MACHINE implementation — 2026-02-15
  - [x] 13.1.1 ADD STATE: add to states set, validate no duplicate
  - [x] 13.1.2 REMOVE STATE + MIGRATE: move instances, remove transitions, clean up ANY except
  - [x] 13.1.3 ADD TRANSITION: add to transition map, validate states exist
  - [x] 13.1.4 REMOVE TRANSITION: remove from map
  - [x] 13.1.5 MODIFY TRANSITION: replace matching transition (guards/actions/timeout)
  - [x] 13.1.6 ADD DATA field: add with optional backfill or default value
  - [x] 13.1.7 REMOVE DATA field: remove from definition + instances
  - [x] 13.1.8 BACKFILL: evaluate expression and set on all instances
- [x] 13.2 Migration safety checks — 2026-02-15
  - [x] 13.2.1 Cannot remove a state that instances are in without MIGRATE clause (always required)
  - [x] 13.2.2 Cannot remove initial state
  - [x] 13.2.3 Warning when adding a REQUIRED field without DEFAULT or BACKFILL (error)
  - [x] 13.2.4 Validate states exist for transitions, fields exist for backfill
  - [x] 13.2.5 Sequential validation per operation (supports multi-op ALTER)
- [x] 13.3 Version tracking in catalog (auto-increment on update)
- [x] 13.4 Storage migration methods (migrate_instances_state, bulk_update_instances)
- [x] 13.5 Server handler (POST /execute with ALTER MACHINE SMQL)
- [x] 13.6 CLI handler (REPL + exec support)
- [x] 13.7 Tests — 28 tests (engine: 25 alter + 3 parser integration + 2 storage)
- [x] 13.8 CHECKPOINT: All 401 tests pass (373 existing + 28 new) — 2026-02-15

## Phase 14: Integration Tests & Examples [STATUS: COMPLETE]

- [x] 14.1 Support Ticket end-to-end scenario — 16 tests — 2026-02-15
  - [x] 14.1.1 Define machine from .smql, spawn tickets, full lifecycle (open→triaged→in_progress→resolved)
  - [x] 14.1.2 Guard failure testing (missing assignee, wrong actor)
  - [x] 14.1.3 Timeout registration, reopen flow, MEMO in trail
  - [x] 14.1.4 FIND by state, AGGREGATE by state, PATHS, FUNNEL queries
  - [x] 14.1.5 Multiple tickets with diverse paths for analysis
- [x] 14.2 E-Commerce Order scenario (composition) — 17 tests — 2026-02-15
  - [x] 14.2.1 Order with LineItems and Shipment (three-machine composition)
  - [x] 14.2.2 Parent-child spawn and verification
  - [x] 14.2.3 ALL(items, STATE IS confirmed) guard for order fulfillment
  - [x] 14.2.4 CASCADE cancellation to children
  - [x] 14.2.5 Full fulfillment flow (draft→placed→paid→fulfilled→shipped→delivered)
  - [x] 14.2.6 Shipment guards (tracking/carrier IS SET)
  - [x] 14.2.7 Wildcard transitions (EXCEPT FROM for LineItem)
- [x] 14.3 CI/CD Pipeline scenario (three-level composition) — 13 tests — 2026-02-15
  - [x] 14.3.1 Pipeline → Stage → Job three-level hierarchy
  - [x] 14.3.2 ALL/ANY guards (stage passes when ALL jobs pass, fails when ANY fails)
  - [x] 14.3.3 Pipeline passes/fails based on stage states
  - [x] 14.3.4 CASCADE cancel propagation through three levels
  - [x] 14.3.5 FIND/AGGREGATE queries across hierarchy
- [ ] 14.4 Performance benchmarks (deferred)
- [x] 14.5 CHECKPOINT: All 447 tests pass (401 existing + 46 integration) — 2026-02-15

## Phase 15: SDK & Developer Experience Polish [STATUS: COMPLETE]

- [x] 15.1 Workspace setup — 2026-02-16
  - [x] 15.1.1 reqwest + url workspace deps added
  - [x] 15.1.2 smql-sdk deps updated (reqwest, tokio-tungstenite, futures-util, url)
  - [x] 15.1.3 smql-codegen crate created (smql-ast, smql-parser, thiserror deps)
- [x] 15.2 smql-sdk crate — 28 tests (12 unit + 15 integration + 1 doc)
  - [x] 15.2.1 SdkError enum (Http, Server, TransitionDenied, NotFound, Parse, Subscription, Deserialize)
  - [x] 15.2.2 Response types (ExecuteResponse, InstanceResponse, TransitionResponse, MachineInfo, TrailEntryResponse, SdkEvent)
  - [x] 15.2.3 SMQL formatter (format_spawn, format_transition, format_find, value_to_smql)
  - [x] 15.2.4 SmqlClient (new, builder, execute, define_machine, spawn, transition, try_transition, get_instance, list_machines, get_machine, trail, health)
  - [x] 15.2.5 FindBuilder (in_state, where_clause, sort_by, limit, offset, execute, count)
  - [x] 15.2.6 AggregateBuilder (measure, group_by_state, group_by_field, execute)
  - [x] 15.2.7 Subscription (WebSocket connect, next_event, on_event callback, cancel)
  - [x] 15.2.8 Typed API (SmqlMachine, SmqlState traits, TypedInstance, spawn_typed, find_typed)
  - [x] 15.2.9 Prelude module re-exports
  - [x] 15.2.10 Integration tests (15 tests against in-process server)
- [x] 15.3 smql-codegen crate — 11 tests
  - [x] 15.3.1 CodeGenerator (from_source, from_files, generate_rust, generate_combined_rust)
  - [x] 15.3.2 Type mapping (SMQL types → Rust types)
  - [x] 15.3.3 Rust code generation (state enum, data struct, enum fields, machine marker)
  - [x] 15.3.4 PascalCase/snake_case name conversion
  - [x] 15.3.5 Codegen tests (11 tests: type mapping, state enum, data struct, enum field, machine impl, multi-machine, roundtrips)
- [x] 15.4 CLI codegen subcommand (`smql codegen --input dir/ --output src/generated/`)
- [x] 15.5 Bug fix: server route params `:name`/`:id` (axum 0.7 syntax, was using 0.8 `{name}` syntax)
- [x] 15.6 Documentation (README.md, SDK example, rustdoc)
- [x] 15.7 CHECKPOINT: All 530 tests pass (486 base + 44 rocksdb) — 2026-02-16

## Bugfixes [STATUS: COMPLETE]

- [x] BF-1 Wire `engine.wire_callback()` in server and CLI — 2026-02-16
  - **Root cause**: `EngineCallbackImpl` (SIGNAL PARENT / SPAWN CHILD) was fully implemented but `wire_callback()` was never called outside engine unit tests, so both actions silently logged a warning and skipped in server/CLI/REPL.
  - [x] BF-1.1 `SmqlServer::new()` — call `engine.wire_callback()` after `Engine::new()`
  - [x] BF-1.2 `SmqlServer::with_storage()` — call `engine.wire_callback()` after `Engine::new()`
  - [x] BF-1.3 `SmqlServer::with_engine()` — call `engine.wire_callback()` on the Arc'd engine
  - [x] BF-1.4 `run_statements()` in smql-cli — call `engine.wire_callback()` after `Engine::new()`
  - [x] BF-1.5 `run_repl_with_storage()` — call `engine.wire_callback()` after `Engine::new()`
  - [x] BF-1.6 `run_repl()` — call `engine.wire_callback()` after `Engine::new()`
  - [x] BF-1.7 Integration test: `signal_parent_from_shipment_delivery` in test_order_flow.rs (Shipment dispatched→in_transit fires SIGNAL PARENT TO shipped on parent Order)
  - [x] BF-1.8 SDK integration test: `test_signal_parent_via_server` exercises SIGNAL PARENT through the HTTP API
  - [x] BF-1.9 CHECKPOINT: All 932 tests pass (888 base + 44 rocksdb) — 2026-02-16
