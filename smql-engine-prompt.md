# SMQL Database Engine — Claude Code Build Prompt

You are the lead architect and implementation team for **SMQL Engine** — a state machine database written in Rust. This document is your complete specification, coordination system, and session continuity mechanism. Read it fully before doing anything.

---

## SESSION CONTINUITY SYSTEM

**This is the most important section. Read it first.**

All progress is tracked in a file called `CHECKLIST.md` at the project root (`smql-engine/CHECKLIST.md`). Before doing ANY work:

1. Read `CHECKLIST.md` to understand what has been completed.
2. Find the next unchecked item (`- [ ]`).
3. Work on that item.
4. When finished, mark it `- [x]` with a timestamp and brief note.
5. If you encounter a blocker, add a `⚠️ BLOCKED:` note under the item.
6. If you discover new work needed, add sub-items under the current phase.

**At the start of every new session**, do this:

```
1. cat smql-engine/CHECKLIST.md
2. Identify current phase and next incomplete task
3. Read any NOTES.md files in the relevant crate directories for context left by previous sessions
4. Resume work
```

**At the end of every session** (or when you sense the context window is getting long):

```
1. Update CHECKLIST.md with progress
2. Write/update NOTES.md in any crate you modified with:
   - What was done
   - What's partially complete
   - Any decisions made and why
   - Known issues or open questions
3. Commit everything with a descriptive message
```

---

## PROJECT INITIALIZATION

**Do this first if `smql-engine/` does not exist:**

```bash
mkdir -p smql-engine
cd smql-engine
cargo init --name smql-engine
```

Then convert to a Cargo workspace. The project structure must be:

```
smql-engine/
├── CHECKLIST.md              # Master progress tracker (create from Phase list below)
├── ARCHITECTURE.md           # Living architecture doc (update as you go)
├── Cargo.toml                # Workspace root
├── crates/
│   ├── smql-parser/          # SMQL language parser (pest/nom)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── NOTES.md          # Session notes for this crate
│   ├── smql-ast/             # Abstract syntax tree types
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── NOTES.md
│   ├── smql-catalog/         # Machine definitions, schema registry
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── NOTES.md
│   ├── smql-engine/          # Core state machine execution engine
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── NOTES.md
│   ├── smql-storage/         # Pluggable storage backends
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── NOTES.md
│   ├── smql-query/           # Query planner and executor
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── NOTES.md
│   ├── smql-trail/           # Immutable transition trail (event log)
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── NOTES.md
│   ├── smql-timer/           # Timeout and scheduled transition manager
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── NOTES.md
│   ├── smql-hooks/           # Action/hook execution runtime
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── NOTES.md
│   ├── smql-server/          # TCP/HTTP server, wire protocol
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── NOTES.md
│   ├── smql-cli/             # CLI client and REPL
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── NOTES.md
│   └── smql-sdk/             # Client SDK library
│       ├── Cargo.toml
│       ├── src/
│       └── NOTES.md
├── tests/                    # Integration tests
│   ├── test_support_ticket.rs
│   ├── test_order_flow.rs
│   └── test_queries.rs
└── examples/                 # Example machine definitions
    ├── support_ticket.smql
    ├── order.smql
    └── ci_pipeline.smql
```

---

## MASTER CHECKLIST

Create this as `smql-engine/CHECKLIST.md`:

```markdown
# SMQL Engine — Build Checklist

> Last updated: [TIMESTAMP]
> Current phase: Phase 1
> Current agent focus: [AGENT NAME]

---

## Phase 1: Foundation & Data Model [STATUS: NOT STARTED]

- [ ] 1.1 Initialize Cargo workspace with all crates (empty libs)
- [ ] 1.2 Define core types in smql-ast
  - [ ] 1.2.1 MachineDefinition struct (name, states, initial, terminals)
  - [ ] 1.2.2 StateDefinition struct (name, metadata)
  - [ ] 1.2.3 TransitionDefinition struct (from, to, guards, actions, timeout, mutates)
  - [ ] 1.2.4 DataFieldDefinition struct (name, type, constraints)
  - [ ] 1.2.5 Expression enum (for guards — binary ops, field access, literals, function calls)
  - [ ] 1.2.6 Action enum (Notify, Log, Emit, Webhook, SpawnChild)
  - [ ] 1.2.7 Query AST nodes (Find, Get, Aggregate, Trail, Paths, Funnel)
  - [ ] 1.2.8 Command AST nodes (Spawn, Transition, TryTransition, BatchTransition, AlterMachine)
  - [ ] 1.2.9 Value enum (Text, Int, Float, Bool, Uuid, DateTime, Duration, List, Map, Null, Ref)
  - [ ] 1.2.10 TypeDefinition enum (all SMQL types: Text, Int, Float, Bool, Uuid, Date, DateTime, Duration, Enum, Ref, List, Set, Map, Blob, Money, Json)
- [ ] 1.3 Set up error types crate-wide (thiserror)
  - [ ] 1.3.1 SmqlError enum with variants: ParseError, ValidationError, TransitionDenied, GuardFailed, SpawnRejected, QueryError, StorageError, TimeoutError
  - [ ] 1.3.2 TransitionDeniedError with structured guard failure details (guard_expr, actual_value, hint)
- [ ] 1.4 Write unit tests for all core types (ser/de, Display, Clone, PartialEq)
- [ ] 1.5 CHECKPOINT: `cargo build` succeeds, all tests pass

## Phase 2: SMQL Parser [STATUS: NOT STARTED]

- [ ] 2.1 Choose parser strategy (recommend: `winnow` or `chumsky` for good error messages)
- [ ] 2.2 Implement lexer/tokenizer
  - [ ] 2.2.1 Keywords (DEFINE, MACHINE, STATES, TRANSITION, SPAWN, FIND, etc.)
  - [ ] 2.2.2 Identifiers, string literals, numeric literals, duration literals (24h, 7d, 30m)
  - [ ] 2.2.3 Operators (==, !=, >, <, >=, <=, AND, OR, NOT, IN, IS SET, IS NOT SET)
  - [ ] 2.2.4 Punctuation ({, }, (, ), ->, :, ,)
  - [ ] 2.2.5 Comments (-- line comments)
- [ ] 2.3 Implement DEFINE MACHINE parser
  - [ ] 2.3.1 Parse DATA block with types and constraints
  - [ ] 2.3.2 Parse STATES block
  - [ ] 2.3.3 Parse INITIAL STATE / TERMINAL STATES
  - [ ] 2.3.4 Parse TRANSITIONS block with guards, actions, mutates, timeouts
  - [ ] 2.3.5 Parse CHILDREN block
  - [ ] 2.3.6 Parse HOOKS block
  - [ ] 2.3.7 Parse ROLES block
  - [ ] 2.3.8 Parse wildcard transitions (ANY -> state, GROUP -> state)
- [ ] 2.4 Implement command parsers
  - [ ] 2.4.1 SPAWN parser (with THEN TRANSITION, BATCH variants)
  - [ ] 2.4.2 TRANSITION parser (with WITH, MEMO, AS, THROUGH, TRY, batch)
  - [ ] 2.4.3 ALTER MACHINE parser (ADD/REMOVE STATE, MODIFY TRANSITION, ADD DATA, BACKFILL)
- [ ] 2.5 Implement query parsers
  - [ ] 2.5.1 GET parser
  - [ ] 2.5.2 FIND parser with WHERE, SORT, LIMIT, OFFSET
  - [ ] 2.5.3 State-aware predicates (STUCK_IN, TIMEOUT_REMAINING, HAS_VISITED, NEVER_VISITED, ALIVE, TERMINATED)
  - [ ] 2.5.4 TRAIL parser
  - [ ] 2.5.5 AGGREGATE parser with MEASURE, GROUP BY
  - [ ] 2.5.6 PATHS parser
  - [ ] 2.5.7 FUNNEL parser
  - [ ] 2.5.8 COMPARE PATHS parser
- [ ] 2.6 Implement expression parser (guards, WHERE clauses)
  - [ ] 2.6.1 Arithmetic: +, -, *, /
  - [ ] 2.6.2 Comparison: ==, !=, <, >, <=, >=
  - [ ] 2.6.3 Logical: AND, OR, NOT
  - [ ] 2.6.4 Field access: dot notation (a.b.c), SELF, ACTOR
  - [ ] 2.6.5 Function calls: elapsed(), elapsed_in_state(), transition_time(), NOW(), TODAY()
  - [ ] 2.6.6 Collection predicates: ALL(), ANY(), COUNT()
  - [ ] 2.6.7 State predicates: STATE IS, STATE IN
  - [ ] 2.6.8 Set membership: IN { ... }
  - [ ] 2.6.9 Null checks: IS SET, IS NOT SET, IS NULL
  - [ ] 2.6.10 Pattern matching: PATTERN(regex)
- [ ] 2.7 Parser error recovery and quality error messages
  - [ ] 2.7.1 Meaningful span-based errors: "Expected state name after '->' on line 14, column 8"
  - [ ] 2.7.2 Suggestions for common mistakes: "Did you mean 'TRANSITION'?" for typos
- [ ] 2.8 Write parser tests for each grammar rule (at least 3 tests per rule: valid, invalid, edge case)
- [ ] 2.9 Parse the example .smql files (support_ticket.smql, order.smql) end-to-end
- [ ] 2.10 CHECKPOINT: Can parse full SMQL files into AST, all tests pass

## Phase 3: Catalog & Validation [STATUS: NOT STARTED]

- [ ] 3.1 Implement MachineCatalog (in-memory registry of machine definitions)
  - [ ] 3.1.1 Register/unregister machines
  - [ ] 3.1.2 Retrieve machine by name
  - [ ] 3.1.3 Validate machine definition on registration
- [ ] 3.2 Machine validation rules
  - [ ] 3.2.1 Initial state must be in STATES set
  - [ ] 3.2.2 Terminal states must be in STATES set
  - [ ] 3.2.3 All transition sources/targets must be in STATES set
  - [ ] 3.2.4 No transitions FROM terminal states (unless explicitly overridden)
  - [ ] 3.2.5 All states must be reachable from initial state (warn on unreachable)
  - [ ] 3.2.6 Detect dead-end states (non-terminal states with no outgoing transitions)
  - [ ] 3.2.7 Guard expressions type-check against DATA fields
  - [ ] 3.2.8 REF targets must reference registered machines
  - [ ] 3.2.9 CHILDREN machines must exist in catalog
  - [ ] 3.2.10 Timeout target states must be valid transitions
- [ ] 3.3 Schema versioning (machine_name:version)
  - [ ] 3.3.1 Auto-increment version on ALTER MACHINE
  - [ ] 3.3.2 Store version history
  - [ ] 3.3.3 Support `smql diff` between versions
- [ ] 3.4 Catalog persistence (serialize to disk, reload on startup)
- [ ] 3.5 CHECKPOINT: Machines can be defined, validated, stored, and retrieved

## Phase 4: Storage Layer [STATUS: NOT STARTED]

- [ ] 4.1 Define Storage trait (pluggable backend interface)
  ```rust
  #[async_trait]
  pub trait Storage: Send + Sync {
      async fn store_instance(&self, instance: &Instance) -> Result<()>;
      async fn get_instance(&self, id: &InstanceId) -> Result<Option<Instance>>;
      async fn find_instances(&self, machine: &str, filter: &Filter) -> Result<Vec<Instance>>;
      async fn update_instance(&self, id: &InstanceId, mutations: &[Mutation]) -> Result<()>;
      async fn delete_instance(&self, id: &InstanceId) -> Result<()>;
      async fn count_by_state(&self, machine: &str) -> Result<HashMap<String, usize>>;
      // Trail operations
      async fn append_trail_entry(&self, entry: &TrailEntry) -> Result<()>;
      async fn get_trail(&self, id: &InstanceId) -> Result<Vec<TrailEntry>>;
      async fn query_trails(&self, machine: &str, filter: &TrailFilter) -> Result<Vec<TrailEntry>>;
  }
  ```
- [ ] 4.2 Implement MemoryStorage (HashMap-based, for development and tests)
  - [ ] 4.2.1 Instance storage with concurrent access (DashMap or RwLock<HashMap>)
  - [ ] 4.2.2 State index (state -> set of instance IDs) for fast state queries
  - [ ] 4.2.3 Trail storage (append-only Vec per instance)
  - [ ] 4.2.4 Full-scan filtering with predicate pushdown
- [ ] 4.3 Implement RocksDB storage backend (persistent, production-grade)
  - [ ] 4.3.1 Key schema design:
    - Instances: `i:{machine}:{id}` -> serialized Instance
    - State index: `s:{machine}:{state}:{id}` -> empty (existence = membership)
    - Trail: `t:{machine}:{id}:{sequence}` -> serialized TrailEntry
    - Catalog: `c:{machine_name}` -> serialized MachineDefinition
  - [ ] 4.3.2 Column families: instances, state_index, trails, catalog, timers
  - [ ] 4.3.3 Atomic transitions: WriteBatch for (update instance + update state index + append trail)
  - [ ] 4.3.4 Prefix iteration for efficient queries (all instances of a machine, all in a state)
  - [ ] 4.3.5 Compaction and TTL for old trail entries (configurable retention)
- [ ] 4.4 Instance data model
  ```rust
  pub struct Instance {
      pub id: InstanceId,
      pub machine: String,
      pub state: String,
      pub data: HashMap<String, Value>,
      pub created_at: DateTime<Utc>,
      pub updated_at: DateTime<Utc>,
      pub state_entered_at: DateTime<Utc>,
      pub trail_length: u64,
      pub version: u64,  // optimistic concurrency
  }
  ```
- [ ] 4.5 Write storage integration tests (same test suite runs against Memory and RocksDB)
- [ ] 4.6 CHECKPOINT: Can store/retrieve/query instances with both backends

## Phase 5: Core Engine — Spawn & Transition [STATUS: NOT STARTED]

- [ ] 5.1 Implement Engine struct (orchestrates catalog + storage + timer)
  ```rust
  pub struct Engine {
      catalog: Arc<MachineCatalog>,
      storage: Arc<dyn Storage>,
      timer_manager: Arc<TimerManager>,
      hook_executor: Arc<HookExecutor>,
  }
  ```
- [ ] 5.2 Implement SPAWN
  - [ ] 5.2.1 Validate data against machine's DATA definition (types, required fields, constraints)
  - [ ] 5.2.2 Apply DEFAULT values for missing optional fields
  - [ ] 5.2.3 Generate instance ID (prefixed: `{machine_short}_{ulid}`)
  - [ ] 5.2.4 Set state to INITIAL STATE
  - [ ] 5.2.5 Create initial trail entry (spawn event)
  - [ ] 5.2.6 Store atomically (instance + trail entry)
  - [ ] 5.2.7 Execute ON SPAWN hooks if defined
  - [ ] 5.2.8 Handle SPAWN ... THEN TRANSITION TO (spawn + immediate transition)
  - [ ] 5.2.9 Handle SPAWN BATCH (bulk insert with validation)
- [ ] 5.3 Implement TRANSITION
  - [ ] 5.3.1 Load instance (with optimistic locking via version field)
  - [ ] 5.3.2 Verify current_state -> target_state is a declared transition (check direct + wildcard + group)
  - [ ] 5.3.3 Apply WITH mutations to instance data (temporary, for guard evaluation)
  - [ ] 5.3.4 Evaluate ALL guard conditions
    - [ ] Build evaluation context: SELF (instance data), ACTOR (transition performer), functions (elapsed(), NOW(), etc.)
    - [ ] Evaluate each guard expression against context
    - [ ] On failure: collect ALL failures (not just first), return TransitionDenied with structured details
  - [ ] 5.3.5 Apply MUTATE clauses from transition definition
  - [ ] 5.3.6 Update instance: new state, updated_at, state_entered_at, increment version
  - [ ] 5.3.7 Create trail entry (from_state, to_state, actor, timestamp, memo, data_snapshot)
  - [ ] 5.3.8 Atomic storage write (update instance + update state indices + append trail)
  - [ ] 5.3.9 Cancel any existing timeout for the previous state
  - [ ] 5.3.10 Register new timeout if target state's transition has a TIMEOUT clause
  - [ ] 5.3.11 Fire ACTION side effects asynchronously (tokio::spawn)
  - [ ] 5.3.12 Handle TRY TRANSITION (no error on guard failure, returns Result<bool>)
  - [ ] 5.3.13 Handle TRANSITION ... OR STAY (apply data mutations even if transition fails)
  - [ ] 5.3.14 Handle TRANSITION ... THROUGH [states] (sequential multi-hop, stop on first failure)
  - [ ] 5.3.15 Handle TRANSITION ALL ... WHERE (batch transitions with filter)
- [ ] 5.4 Guard expression evaluator
  - [ ] 5.4.1 Build an evaluator that walks the Expression AST
  - [ ] 5.4.2 Field resolution: `SELF.field`, `ACTOR.field`, `ACTOR.role`, nested dot access
  - [ ] 5.4.3 Built-in functions: elapsed(), elapsed_in_state(), elapsed_since(state), NOW(), TODAY()
  - [ ] 5.4.4 Collection functions: ALL(children, predicate), ANY(children, predicate), COUNT()
  - [ ] 5.4.5 Comparison operators with type coercion where sensible (Int vs Float)
  - [ ] 5.4.6 IS SET / IS NOT SET for null checks
  - [ ] 5.4.7 IN { set } for membership checks
  - [ ] 5.4.8 Duration arithmetic: elapsed() > 24h
- [ ] 5.5 Write comprehensive transition tests
  - [ ] 5.5.1 Happy path: valid transition with passing guards
  - [ ] 5.5.2 Guard failure: structured error with all failures listed
  - [ ] 5.5.3 Invalid transition: state A has no path to state B
  - [ ] 5.5.4 Wildcard transitions (ANY -> state)
  - [ ] 5.5.5 Concurrent transitions on same instance (optimistic lock conflict)
  - [ ] 5.5.6 THROUGH multi-hop with partial failure
  - [ ] 5.5.7 TRY TRANSITION semantics
  - [ ] 5.5.8 Data mutations with WITH clause
  - [ ] 5.5.9 Timeout registration after transition
- [ ] 5.6 CHECKPOINT: Full spawn and transition lifecycle works, all tests pass

## Phase 6: Timer & Timeout System [STATUS: NOT STARTED]

- [ ] 6.1 Implement TimerManager
  - [ ] 6.1.1 Timer registration: (instance_id, state, deadline, target_state)
  - [ ] 6.1.2 Timer cancellation: cancel by (instance_id, state) when instance leaves state
  - [ ] 6.1.3 Timer storage: persist to storage backend (survive restarts)
  - [ ] 6.1.4 Timer wheel or priority queue for efficient "what fires next?" lookups
- [ ] 6.2 Background timer thread/task
  - [ ] 6.2.1 Tokio interval that checks for expired timers (configurable check interval, default 1s)
  - [ ] 6.2.2 On expiry: perform TRANSITION as System actor
  - [ ] 6.2.3 Handle race condition: instance already transitioned before timeout fires
  - [ ] 6.2.4 Retry logic for failed timeout transitions
- [ ] 6.3 DWELL hooks (ON DWELL(state, > duration) triggers)
  - [ ] 6.3.1 Register dwell checks alongside timeout timers
  - [ ] 6.3.2 Dwell fires hooks but does NOT auto-transition (unlike timeout)
- [ ] 6.4 TIMEOUT_REMAINING query function
  - [ ] 6.4.1 Calculate remaining time from timer registry
  - [ ] 6.4.2 Return None for instances without active timeout
- [ ] 6.5 Write timer tests (timeouts fire, cancellation works, restart persistence)
- [ ] 6.6 CHECKPOINT: Timeouts and dwell triggers work correctly

## Phase 7: Query Engine [STATUS: NOT STARTED]

- [ ] 7.1 Query planner
  - [ ] 7.1.1 Parse query AST into logical plan
  - [ ] 7.1.2 Optimize: push state filters to state index (avoid full scan)
  - [ ] 7.1.3 Optimize: push simple field filters to storage layer
  - [ ] 7.1.4 Handle SORT, LIMIT, OFFSET at the plan level
- [ ] 7.2 Implement FIND queries
  - [ ] 7.2.1 STATE IS / STATE IN filters (use state index)
  - [ ] 7.2.2 Data field filters (==, !=, <, >, <=, >=, IN, IS SET)
  - [ ] 7.2.3 STUCK_IN(state, > duration): state index + filter by state_entered_at
  - [ ] 7.2.4 TIMEOUT_REMAINING < duration: query timer manager
  - [ ] 7.2.5 HAS_VISITED(state): scan trail for state occurrence
  - [ ] 7.2.6 NEVER_VISITED(state): scan trail, negate
  - [ ] 7.2.7 ALIVE: state NOT IN terminal_states
  - [ ] 7.2.8 TERMINATED: state IN terminal_states
  - [ ] 7.2.9 TRAIL CONTAINS (pattern): sequential pattern match on trail
  - [ ] 7.2.10 Compound filters with AND/OR/NOT
- [ ] 7.3 Implement GET queries (single instance by ID)
- [ ] 7.4 Implement TRAIL queries
  - [ ] 7.4.1 TRAIL OF instance: return full trail
  - [ ] 7.4.2 Trail filtering by actor, time range, state
  - [ ] 7.4.3 TRAIL.count(state): count visits to a state
- [ ] 7.5 Implement temporal query functions
  - [ ] 7.5.1 elapsed_in_state(): NOW - state_entered_at
  - [ ] 7.5.2 entered_state_at(): state_entered_at field
  - [ ] 7.5.3 duration_in(state): from trail, sum time spent in state
  - [ ] 7.5.4 total_lifecycle_duration(): trail last entry timestamp - created_at
  - [ ] 7.5.5 transition_time(state_a, state_b): from trail, time between first entry to A and first entry to B
- [ ] 7.6 Implement AGGREGATE queries
  - [ ] 7.6.1 COUNT, SUM, AVG, MIN, MAX, PERCENTILE
  - [ ] 7.6.2 GROUP BY (state, data fields, time buckets)
  - [ ] 7.6.3 MEASURE clause evaluation
- [ ] 7.7 Implement PATHS query
  - [ ] 7.7.1 Extract state sequences from trails
  - [ ] 7.7.2 Group by unique path
  - [ ] 7.7.3 Count occurrences, calculate avg duration per path
  - [ ] 7.7.4 COMPARE PATHS ... SEGMENT BY field
- [ ] 7.8 Implement FUNNEL query
  - [ ] 7.8.1 Given ordered states, calculate conversion rate at each step
  - [ ] 7.8.2 Drop-off analysis
- [ ] 7.9 Query result formatting (tabular output, JSON output)
- [ ] 7.10 Write query tests (at least 5 tests per query type)
- [ ] 7.11 CHECKPOINT: All query types work against test data

## Phase 8: Hooks & Actions [STATUS: NOT STARTED]

- [ ] 8.1 Implement HookExecutor
  - [ ] 8.1.1 Async action dispatch (tokio channels)
  - [ ] 8.1.2 Action handlers: LOG (write to structured log), EMIT (internal event bus), NOTIFY (pluggable)
  - [ ] 8.1.3 WEBHOOK action: async HTTP POST with retry (3 attempts, exponential backoff)
  - [ ] 8.1.4 SPAWN action: create child instance in another machine
- [ ] 8.2 Global hooks (BEFORE EACH TRANSITION, AFTER EACH TRANSITION)
  - [ ] 8.2.1 BEFORE hooks can reject (add to guard evaluation pipeline)
  - [ ] 8.2.2 AFTER hooks are fire-and-forget
- [ ] 8.3 ON ENTER / ON EXIT state hooks
- [ ] 8.4 Event bus for EMIT actions (in-process pub/sub)
  - [ ] 8.4.1 SUBSCRIBE TO machine.transitions WHERE filter DELIVER TO WEBHOOK
  - [ ] 8.4.2 In-memory subscribers for inter-machine signals
- [ ] 8.5 SIGNAL implementation (cross-machine transition triggers)
- [ ] 8.6 Write hook tests
- [ ] 8.7 CHECKPOINT: Hooks fire correctly, webhooks retry, signals work

## Phase 9: Machine Composition [STATUS: NOT STARTED]

- [ ] 9.1 Parent-child relationships
  - [ ] 9.1.1 CHILDREN declaration in machine definition
  - [ ] 9.1.2 PARENT reference in child machine
  - [ ] 9.1.3 SPAWN child: link parent_id, store in child machine
  - [ ] 9.1.4 Guards that reference children: ALL(items, STATE IS confirmed)
  - [ ] 9.1.5 SIGNAL PARENT TO TRANSITION (child completion triggers parent)
- [ ] 9.2 CASCADE transitions
  - [ ] 9.2.1 TRANSITION ... CASCADE: transition children to matching terminal state
  - [ ] 9.2.2 Cascade ordering: children first, then parent (or configurable)
- [ ] 9.3 Cross-machine queries
  - [ ] 9.3.1 WHERE ANY(children_ref, predicate)
  - [ ] 9.3.2 WHERE PARENT(Machine).field == value
- [ ] 9.4 Write composition tests (Order -> LineItem -> Shipment scenario)
- [ ] 9.5 CHECKPOINT: Parent-child flows work end-to-end

## Phase 10: Server & Wire Protocol [STATUS: NOT STARTED]

- [ ] 10.1 TCP server (tokio, custom binary protocol for performance)
  - [ ] 10.1.1 Connection handling with tokio::net::TcpListener
  - [ ] 10.1.2 Wire protocol: length-prefixed frames, msgpack or bincode serialized
  - [ ] 10.1.3 Request types: Execute(SMQL string), Prepare(SMQL string), ExecutePrepared(id, params)
  - [ ] 10.1.4 Response types: Ok(result), Error(structured error), Stream(for subscriptions)
  - [ ] 10.1.5 Connection pooling support
- [ ] 10.2 HTTP/REST API (axum, for convenience and tooling)
  - [ ] 10.2.1 POST /query — execute SMQL query, return JSON
  - [ ] 10.2.2 POST /machines — DEFINE MACHINE
  - [ ] 10.2.3 POST /machines/{name}/instances — SPAWN
  - [ ] 10.2.4 POST /machines/{name}/instances/{id}/transition — TRANSITION
  - [ ] 10.2.5 GET /machines/{name}/instances/{id} — GET instance
  - [ ] 10.2.6 GET /machines/{name}/instances/{id}/trail — TRAIL
  - [ ] 10.2.7 GET /machines — list all machines
  - [ ] 10.2.8 GET /health — health check
  - [ ] 10.2.9 WebSocket endpoint for SUBSCRIBE streams
- [ ] 10.3 Authentication & authorization middleware
  - [ ] 10.3.1 Actor identification from request (token -> Actor struct)
  - [ ] 10.3.2 Role resolution (Actor -> roles for a machine)
  - [ ] 10.3.3 Enforce ROLES definitions on spawn/transition/query
- [ ] 10.4 Server configuration (TOML config file)
  ```toml
  [server]
  tcp_port = 5432
  http_port = 8080
  max_connections = 1000

  [storage]
  backend = "rocksdb"
  path = "./data"

  [timers]
  check_interval = "1s"

  [logging]
  level = "info"
  format = "json"
  ```
- [ ] 10.5 Graceful shutdown (drain connections, flush storage, persist timers)
- [ ] 10.6 Write server integration tests
- [ ] 10.7 CHECKPOINT: Server accepts connections, executes SMQL, returns results

## Phase 11: CLI & REPL [STATUS: NOT STARTED]

- [ ] 11.1 CLI binary (clap for argument parsing)
  - [ ] 11.1.1 `smql connect <host:port>` — connect to server
  - [ ] 11.1.2 `smql apply <file.smql>` — send DEFINE MACHINE to server
  - [ ] 11.1.3 `smql query "<SMQL>"` — execute one-off query
  - [ ] 11.1.4 `smql trail <instance_id>` — pretty-print trail
  - [ ] 11.1.5 `smql visualize <machine>` — render state diagram (DOT format -> optional graphviz)
  - [ ] 11.1.6 `smql diff <machine>@v1 <machine>@v2` — show schema changes
  - [ ] 11.1.7 `smql dry-run "TRANSITION ..."` — validate without committing
  - [ ] 11.1.8 `smql export <machine> --format json/csv` — export instances
- [ ] 11.2 Interactive REPL (rustyline for line editing, history)
  - [ ] 11.2.1 Multi-line input (detect incomplete statements)
  - [ ] 11.2.2 Syntax highlighting (simple keyword coloring)
  - [ ] 11.2.3 Tab completion for machine names, state names, keywords
  - [ ] 11.2.4 Pretty table output (comfy-table or tabled crate)
  - [ ] 11.2.5 `.help`, `.machines`, `.states <machine>`, `.transitions <machine>` meta-commands
  - [ ] 11.2.6 Timing output: "3 results (12ms)"
- [ ] 11.3 Write CLI tests (integration tests with a test server)
- [ ] 11.4 CHECKPOINT: CLI and REPL work end-to-end

## Phase 12: Observability [STATUS: NOT STARTED]

- [ ] 12.1 Structured logging (tracing crate)
  - [ ] 12.1.1 Span per request (transition, query, spawn)
  - [ ] 12.1.2 JSON structured output
- [ ] 12.2 Metrics (prometheus crate)
  - [ ] 12.2.1 smql_instances_total (gauge, labels: machine, state)
  - [ ] 12.2.2 smql_transitions_total (counter, labels: machine, from, to)
  - [ ] 12.2.3 smql_transition_duration_seconds (histogram, labels: machine, transition)
  - [ ] 12.2.4 smql_state_dwell_seconds (histogram, labels: machine, state)
  - [ ] 12.2.5 smql_guard_failures_total (counter, labels: machine, transition, guard)
  - [ ] 12.2.6 smql_timeout_fires_total (counter, labels: machine, state)
  - [ ] 12.2.7 smql_query_duration_seconds (histogram, labels: query_type)
  - [ ] 12.2.8 GET /metrics endpoint
- [ ] 12.3 Event streaming for SUBSCRIBE
  - [ ] 12.3.1 In-memory broadcast channel per machine
  - [ ] 12.3.2 Filter events by transition type, state, etc.
  - [ ] 12.3.3 WebSocket delivery
  - [ ] 12.3.4 Webhook delivery with retry queue
- [ ] 12.4 CHECKPOINT: Metrics exported, events stream via WebSocket

## Phase 13: Schema Evolution [STATUS: NOT STARTED]

- [ ] 13.1 ALTER MACHINE implementation
  - [ ] 13.1.1 ADD STATE: add to states set, no data migration needed
  - [ ] 13.1.2 REMOVE STATE + MIGRATE: move instances in removed state to target state, record in trail
  - [ ] 13.1.3 ADD TRANSITION: add to transition map
  - [ ] 13.1.4 REMOVE TRANSITION: remove from map (no instance impact)
  - [ ] 13.1.5 MODIFY TRANSITION: update guards/actions/timeout
  - [ ] 13.1.6 ADD DATA field: add with default value, backfill existing instances
  - [ ] 13.1.7 REMOVE DATA field: mark as deprecated (soft delete for trail compatibility)
  - [ ] 13.1.8 BACKFILL: batch update expression across all instances
- [ ] 13.2 Migration safety checks
  - [ ] 13.2.1 Cannot remove a state that instances are in without MIGRATE clause
  - [ ] 13.2.2 Cannot remove a transition that is the only path from active instances
  - [ ] 13.2.3 Warning when adding a REQUIRED field without DEFAULT or BACKFILL
- [ ] 13.3 Version tracking in catalog
- [ ] 13.4 CHECKPOINT: Schema evolution works safely

## Phase 14: Integration Tests & Examples [STATUS: NOT STARTED]

- [ ] 14.1 Support Ticket end-to-end scenario
  - [ ] 14.1.1 Define machine, spawn tickets, transition through full lifecycle
  - [ ] 14.1.2 Test stuck detection, timeout firing, escalation
  - [ ] 14.1.3 Test path analysis and funnel queries
- [ ] 14.2 E-Commerce Order scenario
  - [ ] 14.2.1 Order with LineItems and Shipment (composition)
  - [ ] 14.2.2 Payment signal flow
  - [ ] 14.2.3 Cascade cancellation
- [ ] 14.3 CI/CD Pipeline scenario (bonus example)
  - [ ] 14.3.1 Pipeline -> Stages -> Jobs (three-level composition)
- [ ] 14.4 Performance benchmarks
  - [ ] 14.4.1 Spawn throughput (target: 10k/sec single node)
  - [ ] 14.4.2 Transition throughput (target: 5k/sec single node)
  - [ ] 14.4.3 Query latency: FIND by state (target: <5ms for 100k instances)
  - [ ] 14.4.4 Trail query latency
- [ ] 14.5 CHECKPOINT: All scenarios pass, benchmarks established

## Phase 15: SDK & Developer Experience Polish [STATUS: NOT STARTED]

- [ ] 15.1 Rust client SDK (smql-sdk crate)
  - [ ] 15.1.1 Connection management (pooled TCP connections)
  - [ ] 15.1.2 Typed API: `client.spawn::<SupportTicket>(data)`, `instance.transition("resolved")`
  - [ ] 15.1.3 Query builder: `client.find::<SupportTicket>().stuck_in("triaged", "4h").limit(10)`
  - [ ] 15.1.4 Subscription API: `client.subscribe("SupportTicket").on_transition(|event| { ... })`
- [ ] 15.2 Code generation (smql-codegen)
  - [ ] 15.2.1 Parse .smql file -> generate Rust types (struct per machine, enum per state)
  - [ ] 15.2.2 Generated types have compile-time transition validation
  - [ ] 15.2.3 CLI: `smql codegen --lang rust --input machines/ --output src/generated/`
- [ ] 15.3 Documentation
  - [ ] 15.3.1 README.md with quick start
  - [ ] 15.3.2 Rustdoc on all public APIs
  - [ ] 15.3.3 Example code in examples/ directory
- [ ] 15.4 FINAL CHECKPOINT: Full system works end-to-end, tests pass, examples run
```

---

## AGENT SYSTEM

You will operate as multiple "agents" — specialized focus areas that you switch between. This is not about parallel execution; it's about maintaining clean separation of concerns and knowing which hat you're wearing.

**When working on a task, declare which agent you are acting as at the top of your work.**

### Agent: Architect
**Focus:** System design decisions, crate boundaries, trait definitions, ARCHITECTURE.md
**When to activate:** Starting a new phase, resolving cross-crate design questions, making technology choices
**Output:** Updated ARCHITECTURE.md, trait definitions, type signatures

### Agent: Parser-Dev
**Focus:** smql-parser and smql-ast crates
**When to activate:** Phase 2 (parser), and any time the grammar needs updating
**Output:** Parser code, AST types, parser tests, grammar documentation

### Agent: Engine-Dev
**Focus:** smql-engine, smql-storage, smql-trail, smql-timer crates
**When to activate:** Phases 4-6 (storage, core engine, timers)
**Output:** Engine code, storage implementations, trail system, timer system

### Agent: Query-Dev
**Focus:** smql-query crate
**When to activate:** Phase 7 (query engine)
**Output:** Query planner, query executor, query tests

### Agent: Infra-Dev
**Focus:** smql-server, smql-cli, smql-hooks, observability
**When to activate:** Phases 8, 10-12 (hooks, server, CLI, observability)
**Output:** Server code, CLI code, hook system, metrics

### Agent: QA
**Focus:** Testing, benchmarks, integration scenarios
**When to activate:** At every CHECKPOINT, and Phase 14
**Output:** Test cases, benchmark results, bug reports added to checklist

---

## TECHNICAL SPECIFICATIONS

### Rust Edition & Dependencies

```toml
# Workspace Cargo.toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
edition = "2021"
rust-version = "1.75"

[workspace.dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }
# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# Error handling
thiserror = "1"
anyhow = "1"
# Parsing (choose one based on Phase 2.1 decision)
winnow = "0.6"
# OR
chumsky = "0.9"
# Storage
rocksdb = "0.22"
# HTTP server
axum = "0.7"
tower = "0.4"
# CLI
clap = { version = "4", features = ["derive"] }
rustyline = "14"
# Observability
tracing = "0.1"
tracing-subscriber = "0.3"
prometheus = "0.13"
# Utilities
uuid = { version = "1", features = ["v7", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
dashmap = "5"
bytes = "1"
```

### Key Design Rules

1. **All public APIs must be `async`** — even if the initial implementation is synchronous. This avoids a painful refactor later.

2. **Storage trait is the only I/O boundary** — the engine never touches disk directly. Everything goes through the `Storage` trait. This is what makes backends pluggable.

3. **Trail entries are immutable** — once written, never modified. The trail is an append-only log. This is non-negotiable.

4. **Transitions are the only way to change state** — there is no `UPDATE instance SET state = 'x'`. State changes ONLY happen through the transition pipeline (validate -> guards -> mutate -> store -> trail -> actions).

5. **IDs use ULID** — sortable, unique, timestamp-embedded. Prefixed with a short machine identifier: `tk_01HX...` for tickets, `ord_01HX...` for orders.

6. **Errors are always structured** — never return a bare string error. Every error has a type, context, and where possible, a hint for how to fix it.

7. **Test every crate independently** — `cargo test -p smql-parser` should work in isolation. Integration tests go in the top-level `tests/` directory.

8. **No unwrap() in library code** — always propagate errors with `?`. `unwrap()` is only acceptable in tests and examples.

### Performance Targets

- SPAWN: 10,000 instances/sec (single node, RocksDB backend)
- TRANSITION: 5,000 transitions/sec (single node, with guard evaluation)
- GET by ID: < 1ms (RocksDB)
- FIND by STATE: < 5ms for 100k instances (using state index)
- TRAIL retrieval: < 10ms for 1000-entry trail
- Timer check: < 1ms per cycle (efficient priority queue)

---

## EXAMPLE .SMQL FILES

Create these in `examples/` during Phase 1 for parser testing:

### examples/support_ticket.smql
```
DEFINE MACHINE SupportTicket (

  DATA {
    customer_id    : UUID        -> REQUIRED
    subject        : TEXT        -> REQUIRED, MAX(200)
    description    : TEXT        -> REQUIRED
    priority       : ENUM(low, medium, high, critical) -> DEFAULT(medium)
    assignee       : REF(Agent)  -> OPTIONAL
    tags           : SET(TEXT)   -> DEFAULT({})
    satisfaction   : INT         -> RANGE(1, 5), OPTIONAL
    resolution_note: TEXT        -> OPTIONAL
  }

  STATES {
    open
    triaged
    in_progress
    waiting_on_customer
    resolved
    closed
    reopened
  }

  INITIAL STATE open
  TERMINAL STATES { closed }

  TRANSITIONS {
    open -> triaged {
      GUARD  : assignee IS SET
      ACTION : NOTIFY(assignee, "ticket.assigned")
    }

    triaged -> in_progress {
      GUARD : ACTOR == assignee OR ACTOR.role == "admin"
    }

    in_progress -> waiting_on_customer {
      GUARD   : ACTOR == assignee
      TIMEOUT : 72h -> resolved
      ACTION  : NOTIFY(customer_id, "ticket.needs_response")
    }

    waiting_on_customer -> in_progress {
      GUARD : ACTOR.id == customer_id OR ACTOR == assignee
    }

    in_progress -> resolved {
      GUARD  : resolution_note IS SET
      GUARD  : ACTOR == assignee OR ACTOR.role == "admin"
      TIMEOUT: 7d -> closed
      ACTION : NOTIFY(customer_id, "ticket.resolved")
    }

    resolved -> reopened {
      GUARD : ACTOR.id == customer_id
      GUARD : elapsed_since(resolved) < 30d
    }

    reopened -> in_progress {
      GUARD : assignee IS SET
    }

    resolved -> closed {
      GUARD : elapsed_since(resolved) >= 7d OR ACTOR.role == "admin"
    }

    ANY -> triaged {
      EXCEPT FROM { open, closed }
      GUARD  : ACTOR.role IN ("admin", "supervisor")
      MUTATE : priority = critical
      ACTION : LOG("Escalated by {ACTOR}")
    }
  }
)
```

### examples/order.smql
```
DEFINE MACHINE Order (

  DATA {
    customer : REF(Customer) -> REQUIRED
    total    : MONEY(USD)    -> REQUIRED
    notes    : TEXT           -> OPTIONAL
  }

  STATES { draft, placed, paid, payment_failed, fulfilled, shipped, delivered, cancelled, returned }
  INITIAL STATE draft
  TERMINAL STATES { delivered, cancelled, returned }

  CHILDREN {
    items    : LIST(LineItem)    -> MIN(1)
    shipment : OPTIONAL(Shipment)
  }

  TRANSITIONS {
    draft -> placed {
      GUARD  : items.count > 0
      GUARD  : total > 0
      ACTION : EMIT("order.placed", { order: SELF })
    }

    placed -> paid {
      GUARD  : SIGNAL FROM PaymentProcess WHERE state == "succeeded"
    }

    placed -> payment_failed {
      GUARD  : SIGNAL FROM PaymentProcess WHERE state == "failed"
      TIMEOUT: 24h -> cancelled
    }

    payment_failed -> placed {
      -- retry payment
    }

    paid -> fulfilled {
      GUARD  : ALL(items, STATE IS confirmed)
      MUTATE : shipment = SPAWN Shipment { order: SELF }
    }

    fulfilled -> shipped {
      GUARD : shipment.STATE IS dispatched
    }

    shipped -> delivered {
      GUARD : shipment.STATE IS delivered
    }

    delivered -> returned {
      GUARD : elapsed_since(delivered) < 14d
      ACTION: EMIT("order.returned", { order: SELF })
    }

    ANY -> cancelled {
      EXCEPT FROM { shipped, delivered, returned }
      ACTION : EMIT("order.cancelled", { order: SELF })
    }
  }
)

DEFINE MACHINE LineItem (
  PARENT : Order

  DATA {
    product  : TEXT          -> REQUIRED
    quantity : INT           -> MIN(1), REQUIRED
    price    : MONEY(USD)   -> REQUIRED
  }

  STATES { pending, confirmed, backordered, cancelled }
  INITIAL STATE pending
  TERMINAL STATES { confirmed, cancelled }

  TRANSITIONS {
    pending -> confirmed {
      GUARD : quantity > 0
    }
    pending -> backordered {
      ACTION : NOTIFY(PARENT.customer, "item.backordered")
    }
    backordered -> confirmed {}
    ANY -> cancelled {
      EXCEPT FROM { confirmed }
    }
  }
)

DEFINE MACHINE Shipment (
  PARENT : Order

  DATA {
    tracking : TEXT                       -> OPTIONAL
    carrier  : ENUM(fedex, ups, dhl, usps) -> OPTIONAL
  }

  STATES { created, dispatched, in_transit, delivered, lost }
  INITIAL STATE created
  TERMINAL STATES { delivered, lost }

  TRANSITIONS {
    created -> dispatched {
      GUARD  : tracking IS SET
      GUARD  : carrier IS SET
      ACTION : NOTIFY(PARENT.customer, "order.shipped")
    }
    dispatched -> in_transit {}
    in_transit -> delivered {
      SIGNAL PARENT TO delivered
    }
    in_transit -> lost {
      ACTION : NOTIFY(PARENT.customer, "shipment.lost")
    }
  }
)
```

---

## HOW TO START

1. **Read this entire document.**
2. **Create the project structure** (workspace, all crates as empty libs).
3. **Create CHECKLIST.md** from the checklist above.
4. **Create ARCHITECTURE.md** with initial design decisions.
5. **Begin Phase 1.** Work through items sequentially.
6. **At each CHECKPOINT**, run all tests, fix failures, then update CHECKLIST.md before moving on.
7. **At the end of each session**, update CHECKLIST.md and write NOTES.md in each modified crate.

**The single most important rule: Never start a session without reading CHECKLIST.md first. Never end a session without updating it.**