# SMQL — State Machine Query Language

## The Complete Developer Guide

**Version 0.2.0 — Language Specification & Developer Reference**

---

## 1. Philosophy

SMQL is built on a single conviction: most data has a lifecycle, and your database should understand it.

Every traditional database treats your data as inert — rows sitting in tables, waiting for your application to decide what's valid, what transitions are legal, what should happen next. SMQL flips this. Your data is *alive*. It has states, it moves through transitions, it enforces its own rules, and it remembers every step of its journey.

Three principles guide every design decision in SMQL:

**Principle 1 — Declare intent, not mechanics.** You should describe *what* your business process looks like, not *how* to enforce it. If an order can't be shipped before it's paid, you declare that once. The database enforces it forever.

**Principle 2 — Transitions are first-class citizens.** A state change isn't an UPDATE — it's a meaningful business event with preconditions, side effects, and history. SMQL treats it that way.

**Principle 3 — Time is not an afterthought.** Every state has duration. Every transition has a timestamp. Timeouts, deadlines, and scheduling are native to the language, not bolted on through cron jobs.

---

## 2. Core Concepts

Before writing any SMQL, you need to understand five primitives:

**Machine** — A blueprint that defines a lifecycle. Think of it as the equivalent of a table in SQL, but instead of defining columns, you define states and the rules governing movement between them. Example: `Order`, `SupportTicket`, `Pipeline`.

**Instance** — A single entity living within a machine. If `Order` is the blueprint, then an instance is one specific order with its own state, data, and history. Each instance has a ULID identifier (26-character, time-sortable). Equivalent to a row in SQL.

**State** — A named, stable condition an instance can be in. States are not strings in a column — they are declared, finite, and known to the database at the schema level. The database will reject any attempt to place an instance in an undeclared state.

**Transition** — A declared, legal movement from one state to another. Transitions can have guard conditions (preconditions that must be true), actions (side effects triggered on transition), and timeout rules. Transitions that haven't been declared simply cannot happen — the database refuses them.

**Trail** — The complete, immutable, append-only history of every transition an instance has undergone. Automatically maintained. Never manually written. Queryable as a first-class structure. This is your audit log, your event stream, and your time-travel mechanism — all in one.

---

## 3. Defining Machines

### 3.1 — Your First Machine

```smql
DEFINE MACHINE SupportTicket (

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
)
```

That's a valid machine. Minimal, but it compiles. Every machine needs at least: a name, a set of states, an initial state, and optionally one or more terminal states (states from which no further transitions are expected).

### 3.2 — Adding Data

Instances carry data. You define the data shape inside the machine:

```smql
DEFINE MACHINE SupportTicket (

  DATA {
    customer_id    : UUID        -> REQUIRED
    subject        : TEXT        -> REQUIRED, MAX(200)
    description    : TEXT        -> REQUIRED
    priority       : ENUM(low, medium, high, critical) -> DEFAULT(medium)
    assignee       : REF(Agent)  -> OPTIONAL
    tags           : SET(TEXT)   -> DEFAULT({})
    attachments    : LIST(BLOB)  -> DEFAULT([])
    satisfaction   : INT         -> RANGE(1, 5), OPTIONAL
    resolution_note: TEXT        -> OPTIONAL
  }

  -- states and transitions follow...
)
```

**Type system at a glance:**

| Type | Description | Example |
|------|-------------|---------|
| `TEXT` | UTF-8 string | `"Payment failed"` |
| `INT` | 64-bit integer | `42` |
| `FLOAT` | 64-bit double | `99.95` |
| `BOOL` | Boolean | `TRUE` / `FALSE` |
| `UUID` | Universally unique ID | `a1b2c3d4-...` |
| `DATE` | Calendar date | `2025-03-15` |
| `DATETIME` | Timestamp (UTC) | `2025-03-15T14:30:00Z` |
| `DURATION` | Time span | `48h`, `30m`, `7d`, `60s` |
| `ENUM(...)` | Constrained set of values | `ENUM(low, medium, high)` |
| `REF(Machine)` | Reference to another machine instance | `REF(Customer)` |
| `LIST(Type)` | Ordered collection | `LIST(TEXT)` |
| `SET(Type)` | Unique collection | `SET(UUID)` |
| `MAP(K, V)` | Key-value pairs | `MAP(TEXT, INT)` |
| `BLOB` | Binary large object | File uploads |
| `MONEY(Currency)` | Amount with currency code | `MONEY(USD)` |
| `JSON` | Flexible structure | Schema-free nested data |

**Data constraints:**

```smql
-- Constraints are chained with commas after the -> arrow
customer_id : UUID -> REQUIRED
email       : TEXT -> REQUIRED, PATTERN("^.+@.+\\..+$"), UNIQUE
age         : INT  -> RANGE(0, 150)
score       : FLOAT -> MIN(0.0)
metadata    : JSON -> OPTIONAL
priority    : ENUM(low, medium, high) -> DEFAULT(medium)
```

| Constraint | Description |
|------------|-------------|
| `REQUIRED` | Field must be provided (cannot be null) |
| `OPTIONAL` | Field may be null |
| `DEFAULT(value)` | Default value if not provided |
| `MIN(n)` | Minimum value or length |
| `MAX(n)` | Maximum value or length |
| `RANGE(lo, hi)` | Inclusive numeric range |
| `UNIQUE` | Must be unique across all instances |
| `PATTERN("regex")` | Regex validation on text fields |

### 3.3 — Defining Transitions

This is where SMQL comes alive:

```smql
DEFINE MACHINE SupportTicket (

  DATA {
    customer_id    : UUID        -> REQUIRED
    subject        : TEXT        -> REQUIRED
    priority       : ENUM(low, medium, high, critical) -> DEFAULT(medium)
    assignee       : REF(Agent)  -> OPTIONAL
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
      GUARD   : assignee IS SET
      GUARD   : priority != "low" OR elapsed() < 24h
      ACTION  : NOTIFY(assignee, "ticket.assigned")
      ACTION  : LOG("Ticket triaged")
    }

    triaged -> in_progress {
      GUARD   : ACTOR == assignee OR ACTOR.role == "admin"
      ACTION  : NOTIFY(customer_id, "ticket.in_progress")
    }

    in_progress -> waiting_on_customer {
      GUARD   : ACTOR == assignee
      TIMEOUT : 72h -> resolved
      ACTION  : NOTIFY(customer_id, "ticket.needs_response")
    }

    waiting_on_customer -> in_progress {
      -- Customer responded, back to active work
      GUARD   : ACTOR.id == customer_id OR ACTOR == assignee
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

    -- Wildcard: any non-terminal state can be escalated
    ANY -> triaged {
      EXCEPT FROM { open, closed }
      GUARD  : ACTOR.role IN ("admin", "supervisor")
      MUTATE : priority = "critical"
      ACTION : NOTIFY(assignee, "ticket.escalated")
      ACTION : LOG("Escalated")
    }
  }
)
```

### 3.4 — Transition Anatomy

Every transition has the following optional clauses:

```smql
state_a -> state_b {

  -- GUARD: preconditions that must ALL be true for the transition to fire.
  -- If any guard fails, the transition is rejected with a clear error.
  -- Multiple GUARD clauses are allowed; all must pass.
  GUARD : <boolean expression>

  -- MUTATE: data modifications that happen atomically with the transition.
  -- These are guaranteed changes — not suggestions.
  -- Multiple MUTATE clauses are allowed.
  MUTATE : <field> = <expression>

  -- ACTION: side effects triggered after a successful transition.
  -- Actions are asynchronous and non-blocking by default.
  -- A failed action does NOT roll back the transition.
  -- Multiple ACTION clauses are allowed.
  ACTION : <effect>

  -- TIMEOUT: automatic transition if the instance stays in the
  -- target state for too long. Only one timeout per target state.
  -- Timeout transitions bypass guards (executed as System actor).
  TIMEOUT : <duration> -> <target_state>

  -- EXCEPT FROM: exclude states from wildcard (ANY) transitions.
  -- Only valid when transition source is ANY.
  EXCEPT FROM { state1, state2, ... }

  -- SIGNAL PARENT TO: signal the parent machine to transition.
  -- Only valid in child machines that declare PARENT.
  SIGNAL PARENT TO <state>
}
```

### 3.5 — Wildcard Transitions

For transitions that apply from many states:

```smql
-- From any state (except those excluded)
ANY -> cancelled {
  EXCEPT FROM { shipped, delivered, closed }
  GUARD  : ACTOR.role == "admin" OR ACTOR.id == customer_id
  ACTION : EMIT("order.cancelled", { order: SELF })
}
```

The `ANY` keyword creates a transition from every non-excluded state. `EXCEPT FROM` lists states that should not have this transition.

---

## 4. Creating and Transitioning Instances

### 4.1 — Spawning an Instance

```smql
-- Basic creation. Instance enters the INITIAL STATE automatically.
SPAWN SupportTicket {
  customer_id : "c_9f83a1b2"
  subject     : "Cannot access dashboard"
  description : "Getting 403 error since this morning"
  priority    : "high"
}
-- Returns: instance with a ULID id, IN STATE open
```

```smql
-- Spawn and immediately transition (if guards allow)
SPAWN SupportTicket {
  customer_id : "c_9f83a1b2"
  subject     : "Billing question"
  assignee    : "a_emily"
} THEN TRANSITION TO triaged
-- Returns: instance IN STATE triaged
```

```smql
-- Batch spawn: create multiple instances atomically
SPAWN BATCH SupportTicket [
  { customer_id: "c_001", subject: "Issue A", priority: "low" },
  { customer_id: "c_002", subject: "Issue B", priority: "high" },
  { customer_id: "c_003", subject: "Issue C", priority: "medium" }
]
-- Returns: 3 instances created in STATE open
```

> **Note:** Data fields use colons for assignment: `{ key: value }`. SPAWN requires `{}` even with no data: `SPAWN Machine {}`.

### 4.2 — Performing Transitions

The core write operation in SMQL is `TRANSITION`, not `UPDATE`:

```smql
-- Basic transition
TRANSITION SupportTicket tk_00123 TO triaged
  WITH { assignee: "a_emily" }
  MEMO "Assigned to Emily — she handled a similar issue last week"
  AS "a_supervisor"
```

Let's break this down:

- `TRANSITION Machine instance_id TO state` — the machine name and instance ID are space-separated. The database checks that a valid path exists from the current state.
- `WITH { }` — data mutations bundled with the transition. Uses braces and colon assignment: `{ key: value }`.
- `MEMO "text"` — an optional human-readable note stored in the trail.
- `AS actor` — the actor performing the transition. Used for guard evaluation and audit.

**What happens internally:**

1. Database locks the instance.
2. Verifies current state → target state is a declared transition.
3. Applies `WITH` mutations to the instance data.
4. Evaluates all `GUARD` conditions. If any fail → reject with error.
5. Applies any `MUTATE` clauses from the transition definition.
6. Records the transition in the trail (from_state, to_state, actor, timestamp, memo, data_snapshot).
7. Fires `ACTION` side effects asynchronously.
8. Releases lock.

**The entire sequence (steps 1–6) is atomic.** Actions (step 7) are eventual.

### 4.3 — Conditional Transitions

```smql
-- Only transition if guards pass; otherwise do nothing (no error)
TRY TRANSITION SupportTicket tk_00123 TO resolved
  WITH { resolution_note: "Cleared browser cache" }
  AS "a_emily"
```

```smql
-- OR_STAY: apply mutations even if the transition guard fails.
-- The instance remains in its current state but data is updated.
TRANSITION SupportTicket tk_00123 TO resolved
  WITH { resolution_note: "Fixed" }
  OR_STAY
  AS "a_emily"
```

### 4.4 — Batch Transitions

```smql
-- Transition all matching instances
TRANSITION ALL SupportTicket
  WHERE STATE IS resolved AND elapsed() > 7d
  TO closed
  AS "auto_closer"
```

### 4.5 — Multi-Step Transitions (THROUGH)

Sometimes you need an instance to move through multiple states in one logical operation:

```smql
-- Express checkout: paid → fulfilled → shipped in one call
-- Each intermediate transition's guards are still evaluated
TRANSITION Order ord_555 THROUGH [paid, fulfilled, shipped]
  WITH { tracking: "TRK_998877" }
  AS "express_checkout"
```

The database evaluates each hop sequentially. If any intermediate guard fails, it stops at the last successful state and reports where it halted.

### 4.6 — Cascading Transitions

When transitioning a parent, you can cascade to children:

```smql
-- Cancel an order and all its children
TRANSITION Order ord_001 TO cancelled CASCADE
-- All LineItems → first terminal state, Shipment → first terminal state
```

`CASCADE` recursively transitions all child instances to the first declared terminal state of their respective machines.

---

## 5. Querying

### 5.1 — Basic Queries

```smql
-- Find by ID
GET SupportTicket tk_00123

-- Find by current state
FIND SupportTicket WHERE STATE IS open

-- Compound filters
FIND SupportTicket
  WHERE STATE IS in_progress
    AND priority == "critical"
    AND assignee == "a_emily"
  SORT BY created_at DESC
  LIMIT 20
```

### 5.2 — State-Aware Predicates

These are queries that only make sense in a state machine context — and they're native to SMQL:

```smql
-- Instances currently in a specific state
FIND SupportTicket WHERE STATE IS in_progress

-- Instances currently in any of a set of states
FIND SupportTicket WHERE STATE IN { open, triaged, in_progress }

-- Field presence checks
FIND SupportTicket WHERE assignee IS SET
FIND SupportTicket WHERE resolution_note IS NOT SET

-- Set membership
FIND SupportTicket WHERE priority IN ("high", "critical")
```

### 5.3 — Temporal Queries

Time is a first-class dimension. Every state entry and exit is recorded automatically.

```smql
-- Instances using elapsed time (time in current state)
FIND SupportTicket WHERE elapsed() > 4h

-- Elapsed since entering a specific state
FIND SupportTicket WHERE elapsed_since(resolved) < 30d
```

**Available temporal functions:**

| Function | Description |
|----------|-------------|
| `elapsed()` | Time since entering the current state |
| `elapsed_in_state()` | Alias for `elapsed()` |
| `elapsed_since(state)` | Time since first entry into a named state |
| `NOW()` | Current timestamp |
| `TODAY()` | Current date |
| `timeout_remaining()` | Duration remaining on active timeout (NULL if none) |

### 5.4 — Trail Queries

The trail is the immutable transition history. You can query it directly:

```smql
-- Full trail for an instance
TRAIL OF tk_00123

-- Returns:
-- | seq | from        | to                  | actor         | at                   | memo             |
-- |-----|-------------|---------------------|---------------|----------------------|------------------|
-- | 0   | (spawn)     | open                | (system)      | 2025-03-01T09:00:00Z |                  |
-- | 1   | open        | triaged             | a_supervisor  | 2025-03-01T09:15:00Z | Assigned Emily   |
-- | 2   | triaged     | in_progress         | a_emily       | 2025-03-01T09:20:00Z |                  |
-- | 3   | in_progress | waiting_on_customer | a_emily       | 2025-03-01T10:00:00Z |                  |
-- | 4   | waiting...  | in_progress         | c_01          | 2025-03-01T14:30:00Z |                  |
-- | 5   | in_progress | resolved            | a_emily       | 2025-03-01T15:00:00Z | Cache cleared    |
```

Trail entries include a spawn event at sequence 0 (from_state is empty).

### 5.5 — Sorting, Limiting, and Pagination

```smql
-- Sort results (multiple fields, ASC or DESC)
FIND SupportTicket
  WHERE STATE IS open
  SORT BY priority DESC, created_at ASC
  LIMIT 20

-- Offset-based pagination
FIND SupportTicket SORT BY created_at DESC LIMIT 20 OFFSET 40

-- Cursor-based pagination (recommended for large datasets)
-- Uses ULID keyset pagination — more efficient than OFFSET for deep pages
FIND SupportTicket
  WHERE STATE IS open
  SORT BY created_at DESC
  LIMIT 20
  AFTER "01HWZK4G5C8T3RNMK1VNSH7HYM"
-- The AFTER value is the ULID of the last instance from the previous page.
-- The response includes a next_cursor value for the next page.
```

### 5.6 — Path Analysis

Analyzing the routes instances take through their lifecycle:

```smql
-- Most common paths through the machine
PATHS FROM SupportTicket
  WHERE created_at > "2025-01-01"
  LIMIT 5

-- Returns:
-- | path                                              | count | pct   | avg_duration |
-- |---------------------------------------------------|-------|-------|--------------|
-- | open → triaged → in_progress → resolved → closed  | 4521  | 62.1% | 4.2h         |
-- | open → triaged → in_progress → w_o_c → ... → closed | 1203 | 16.5% | 18.7h       |
-- | open → cancelled                                  | 892   | 12.2% | 0.3h         |
```

```smql
-- Path comparison: how do critical tickets flow vs low priority?
COMPARE PATHS SupportTicket
  SEGMENT BY priority
  WHERE created_at > "2025-01-01"
```

### 5.7 — Funnel Analysis

```smql
-- Conversion funnel: what percentage of instances progress through each stage?
FUNNEL SupportTicket
  THROUGH [open, triaged, in_progress, resolved, closed]
  WHERE created_at > "2025-03-01"
-- Returns drop-off rates at each stage
```

### 5.8 — Aggregations

```smql
-- State distribution: how many instances in each state right now?
AGGREGATE SupportTicket
  MEASURE COUNT() AS total
  GROUP BY STATE

-- Aggregation with a filter
AGGREGATE SupportTicket
  MEASURE COUNT() AS total, AVG(satisfaction) AS avg_sat
  WHERE STATE IS closed
  GROUP BY priority
```

**Aggregate functions:**

| Function | Description |
|----------|-------------|
| `COUNT()` | Number of matching instances |
| `COUNT(field)` | Number where field is set |
| `SUM(field)` | Sum of numeric field |
| `AVG(field)` | Average of numeric field |
| `MIN(field)` | Minimum value |
| `MAX(field)` | Maximum value |
| `PERCENTILE(p)` | Percentile (0.0–1.0) |

**GROUP BY options:**
- `GROUP BY STATE` — group by current state
- `GROUP BY field` — group by a data field

---

## 6. Machine Composition

### 6.1 — Child Machines

Machines can own other machines, creating parent-child lifecycles:

```smql
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
    -- An order has line items and an optional shipment
    items    : LIST(LineItem)    -> MIN(1)
    shipment : OPTIONAL(Shipment)
  }

  TRANSITIONS {
    draft -> placed {
      GUARD  : items.count > 0
      GUARD  : total > 0
      ACTION : EMIT("order.placed", { order: SELF })
    }

    paid -> fulfilled {
      -- Guard references children's states
      GUARD : ALL(items, STATE IS confirmed)
      MUTATE : shipment = SPAWN Shipment { order: SELF }
    }

    fulfilled -> shipped {
      GUARD : shipment.STATE IS dispatched
    }

    shipped -> delivered {
      GUARD : shipment.STATE IS delivered
    }

    ANY -> cancelled {
      EXCEPT FROM { shipped, delivered, returned }
      ACTION : EMIT("order.cancelled", { order: SELF })
    }
  }
)
```

**CHILDREN cardinality options:**

| Syntax | Meaning |
|--------|---------|
| `LIST(Machine)` | Zero or more children |
| `LIST(Machine) -> MIN(1)` | At least one child |
| `LIST(Machine) -> MIN(1), MAX(5)` | Between 1 and 5 children |
| `OPTIONAL(Machine)` | Zero or one child |
| `Machine` (identifier) | Exactly one child (required) |

### 6.2 — Parent Declaration

Child machines declare their parent:

```smql
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
```

### 6.3 — Signals Between Parent and Child

Child machines can signal their parent to transition:

```smql
DEFINE MACHINE Shipment (
  PARENT : Order

  DATA {
    tracking : TEXT                        -> OPTIONAL
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
      -- When shipment is delivered, signal the parent Order to transition
      SIGNAL PARENT TO delivered
    }
    in_transit -> lost {
      ACTION : NOTIFY(PARENT.customer, "shipment.lost")
    }
  }
)
```

### 6.4 — Cross-Machine Queries

```smql
-- Find all orders where ANY line item is backordered
FIND Order
  WHERE ANY(items, STATE IS backordered)

-- Guard that checks a signal from another machine
paid -> fulfilled {
  GUARD : SIGNAL FROM PaymentProcess WHERE state == "succeeded"
}

-- Cascade: cancel an order and all its children
TRANSITION Order ord_001 TO cancelled CASCADE
-- All LineItems → cancelled, Shipment → cancelled (first terminal state)
```

**Collection predicates for children:**

| Predicate | Description |
|-----------|-------------|
| `ALL(children, predicate)` | True if all children match (true for empty — vacuous truth) |
| `ANY(children, predicate)` | True if any child matches (false for empty) |

---

## 7. Hooks and Side Effects

### 7.1 — Action Types

Actions are side effects triggered by transitions. They execute asynchronously after the transition commits.

```smql
TRANSITIONS {
  open -> triaged {
    -- Log a message
    ACTION : LOG("Triaged ticket")

    -- Emit an internal event (consumed via EventBus / WebSocket)
    ACTION : EMIT("ticket.triaged", { ticket: SELF, actor: ACTOR })

    -- Notify an actor
    ACTION : NOTIFY(assignee, "ticket.assigned")

    -- Webhook: async HTTP POST (retries on 5xx/network errors, no retry on 4xx)
    ACTION : WEBHOOK("https://api.example.com/hooks/ticket-triaged")
    -- Or with a payload:
    ACTION : WEBHOOK("https://api.example.com/hooks", { ticket_id: SELF })

    -- Spawn a child machine instance (in MUTATE context)
    MUTATE : shipment = SPAWN Shipment { order: SELF }
  }
}
```

**Action types:**

| Action | Description |
|--------|-------------|
| `LOG("message")` | Write to structured log |
| `EMIT("event_name")` | Publish event to internal EventBus |
| `EMIT("event_name", payload)` | Publish event with data payload |
| `NOTIFY(target, "event_type")` | Pluggable notification to an actor/channel |
| `WEBHOOK("url")` | HTTP POST to URL |
| `WEBHOOK("url", payload)` | HTTP POST with JSON payload |
| `SPAWN Machine { data }` | Create child instance (used in MUTATE clause) |
| `SIGNAL PARENT TO state` | Tell parent to transition (in child transitions) |

### 7.2 — Global Hooks

Apply logic to all transitions within a machine:

```smql
DEFINE MACHINE SupportTicket (
  -- ...

  HOOKS {
    -- Fires when an instance is created
    ON SPAWN {
      ACTION : EMIT("ticket.created", SELF)
    }

    -- Runs before every transition (CAN reject — treated as guard failure)
    BEFORE EACH TRANSITION {
      ACTION : LOG("Transition attempted")
    }

    -- Runs after every transition (cannot reject, fire-and-forget)
    AFTER EACH TRANSITION {
      ACTION : EMIT("ticket.transitioned", { instance: SELF, actor: ACTOR })
    }

    -- Runs when any instance enters a specific state
    ON ENTER resolved {
      ACTION : NOTIFY(customer_id, "ticket.resolved")
    }

    -- Runs when any instance exits a specific state
    ON EXIT in_progress {
      ACTION : LOG("Left in_progress state")
    }
  }
)
```

**Hook triggers:**

| Trigger | When it fires | Can reject? |
|---------|---------------|-------------|
| `ON SPAWN` | Instance created | No |
| `BEFORE EACH TRANSITION` | Before any transition commits | Yes (treated as guard failure) |
| `AFTER EACH TRANSITION` | After any transition commits | No (fire-and-forget) |
| `ON ENTER state` | When entering a specific state | No |
| `ON EXIT state` | When exiting a specific state | No |

> **Planned:** `ON DWELL(state, > duration)` — fire after dwelling in a state for a specified duration (defined in AST, not yet active).

---

## 8. Schema Evolution

Machines evolve over time. SMQL provides `ALTER MACHINE` to modify live machines safely.

### 8.1 — Adding States and Transitions

```smql
ALTER MACHINE SupportTicket
  ADD STATE on_hold
  ADD TRANSITION in_progress -> on_hold {
    GUARD  : ACTOR.role == "admin"
    TIMEOUT: 7d -> triaged
  }
  ADD TRANSITION on_hold -> in_progress {
    GUARD  : ACTOR.role == "admin"
  }
```

### 8.2 — Removing States

You can't just delete a state if instances are in it. SMQL requires a migration target:

```smql
ALTER MACHINE SupportTicket
  REMOVE STATE on_hold MIGRATE TO triaged
```

All instances currently in `on_hold` are migrated to `triaged`. Transitions referencing the removed state are cleaned up automatically.

### 8.3 — Removing Transitions

```smql
ALTER MACHINE SupportTicket
  REMOVE TRANSITION resolved -> reopened
  -- Customers can no longer reopen resolved tickets
```

### 8.4 — Adding and Removing Data Fields

```smql
-- Add a new data field with optional backfill expression
ALTER MACHINE SupportTicket
  ADD DATA sla_tier : ENUM(standard, premium, enterprise) -> DEFAULT(standard)
    BACKFILL "standard"

-- Remove a data field
ALTER MACHINE SupportTicket
  REMOVE DATA legacy_field

-- Backfill an existing field
ALTER MACHINE SupportTicket
  BACKFILL sla_tier = "premium"
```

### 8.5 — Multi-Operation ALTER

Multiple operations in a single `ALTER MACHINE` are applied sequentially — each operation is validated and applied before the next one executes, so later operations can depend on earlier ones:

```smql
ALTER MACHINE SupportTicket
  ADD STATE escalated
  ADD TRANSITION in_progress -> escalated {}
  ADD DATA escalation_reason : TEXT -> OPTIONAL
```

**Supported ALTER operations:**

| Operation | Syntax |
|-----------|--------|
| Add state | `ADD STATE state_name` |
| Remove state | `REMOVE STATE state_name MIGRATE TO target` |
| Add transition | `ADD TRANSITION from -> to { ... }` |
| Remove transition | `REMOVE TRANSITION from -> to` |
| Add data field | `ADD DATA field : Type -> constraints [BACKFILL expr]` |
| Remove data field | `REMOVE DATA field_name` |
| Backfill data | `BACKFILL field = expression` |

---

## 9. Access Control

### 9.1 — Role-Based Transition Control

The `ROLES` block defines what each role can do within a machine:

```smql
DEFINE MACHINE SupportTicket (
  -- ... DATA, STATES, TRANSITIONS ...

  ROLES {
    customer {
      CAN SPAWN
      CAN TRANSITION [waiting_on_customer, in_progress]
      CAN QUERY
    }

    agent {
      CAN SPAWN
      CAN TRANSITION [open, triaged, in_progress, waiting_on_customer, resolved]
      CAN QUERY
      CAN ALTER
    }

    admin {
      CAN SPAWN
      CAN TRANSITION [open, triaged, in_progress, waiting_on_customer, resolved, closed]
      CAN QUERY
      CAN ALTER
    }
  }
)
```

**Role permissions:**

| Permission | Description |
|------------|-------------|
| `CAN SPAWN` | Can create new instances |
| `CAN TRANSITION [states]` | Can initiate transitions involving listed states |
| `CAN QUERY` | Can query instances |
| `CAN ALTER` | Can alter the machine schema |

### 9.2 — JWT Authentication Middleware

The SMQL server supports JWT-based authentication (feature-gated behind the `auth` flag):

- Algorithm: HS256 (HMAC-SHA256)
- Token: passed via `Authorization: Bearer <token>` header
- Payload: must include `sub` (subject/user ID) and optionally `role`
- Skip paths: `/health` and `/metrics` bypass authentication by default

Enable with the `auth` feature flag:

```bash
cargo run --bin smql --features auth -- serve --bind 127.0.0.1:4200 --jwt-secret "your-secret"
```

---

## 10. HTTP API

The SMQL server exposes a REST API via axum:

### 10.1 — Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/execute` | Execute any SMQL statement |
| `GET` | `/machines` | List all registered machine names |
| `GET` | `/machines/:name` | Get a machine definition |
| `GET` | `/instances/:id` | Get an instance by ULID |
| `GET` | `/health` | Health check (`{"status": "ok"}`) |
| `GET` | `/metrics` | Prometheus metrics (text/plain) |
| `GET` | `/subscribe` | WebSocket event stream |

### 10.2 — Executing SMQL

All SMQL statements are sent via `POST /execute`:

```bash
# Define a machine
curl -X POST http://localhost:4200/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "DEFINE MACHINE Counter ( STATES { idle, running, done } INITIAL STATE idle TERMINAL STATES { done } TRANSITIONS { idle -> running {} running -> done {} } )"}'

# Spawn an instance
curl -X POST http://localhost:4200/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "SPAWN Counter {}"}'

# Transition
curl -X POST http://localhost:4200/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION Counter 01HWZK4G5C8T3RNMK1VNSH7HYM TO running"}'

# Query
curl -X POST http://localhost:4200/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "FIND Counter WHERE STATE IS running LIMIT 10"}'
```

### 10.3 — WebSocket Event Streaming

Connect to `/subscribe` for real-time events:

```
ws://localhost:4200/subscribe?machine=SupportTicket&event=SPAWN
```

Query parameters (both optional):
- `machine` — filter events by machine name
- `event` — filter by event type

Events are delivered as JSON messages over the WebSocket connection.

### 10.4 — Cursor Pagination in Responses

When using `AFTER` cursor pagination, the JSON response includes a `next_cursor` field:

```json
{
  "instances": [...],
  "next_cursor": "01HWZK4G5C8T3RNMK1VNSH7HYM"
}
```

Pass `next_cursor` as the `AFTER` value in your next FIND query.

---

## 11. CLI and REPL

### 11.1 — CLI Commands

```bash
# Start the HTTP server
smql serve --bind 127.0.0.1:4200 --storage memory

# Start an interactive REPL
smql repl --storage memory

# Execute a single SMQL statement
smql exec "SPAWN Counter {}"

# Run a .smql script file
smql run script.smql

# Generate typed Rust code from .smql files
smql codegen --input machines/ --output src/generated/
```

### 11.2 — REPL Features

The REPL provides an interactive environment with:

- Multi-line input (incomplete statement detection)
- Tab completion for keywords
- Pretty table output for query results
- Timing: `N results (Xms)`
- History (via rustyline)

**Meta-commands:**

| Command | Description |
|---------|-------------|
| `.help` | Show available commands |
| `.machines` | List all registered machines |
| `.states <machine>` | Show states for a machine |
| `.transitions <machine>` | Show transitions for a machine |

```
smql> FIND SupportTicket WHERE STATE IS open LIMIT 3
┌─────────────────────────────┬─────────────────────┬──────────┐
│ id                          │ subject             │ priority │
├─────────────────────────────┼─────────────────────┼──────────┤
│ 01HWZK4G5C8T3RNMK1VNSH7HYM │ Login broken        │ critical │
│ 01HWZK5A2D7M4QPNJ3VXRW8FKG │ Billing error       │ high     │
│ 01HWZK6B3E9N5RQOK4WYSX9GLH │ API timeout         │ high     │
└─────────────────────────────┴─────────────────────┴──────────┘
3 results (12ms)
```

---

## 12. Rust SDK

The `smql-sdk` crate provides an ergonomic client for interacting with the SMQL server.

### 12.1 — Connection

```rust
use smql_sdk::prelude::*;

// Simple connection
let client = SmqlClient::new("http://localhost:4200")?;

// With configuration
let client = SmqlClient::builder("http://localhost:4200")
    .timeout(Duration::from_secs(5))
    .build()?;
```

### 12.2 — Machine Operations

```rust
// Define a machine
client.define_machine(r#"
    DEFINE MACHINE Counter (
        STATES { idle, running, done }
        INITIAL STATE idle
        TERMINAL STATES { done }
        TRANSITIONS {
            idle -> running {}
            running -> done {}
        }
    )
"#).await?;

// List machines
let machines = client.list_machines().await?;

// Get machine definition
let machine = client.get_machine("Counter").await?;
```

### 12.3 — Instance Operations

```rust
// Spawn an instance
let inst = client.spawn("Counter", serde_json::json!({})).await?;
println!("Spawned: {} (state: {})", inst.id, inst.state);

// Get an instance
let inst = client.get_instance("01HWZK4G5C8T3RNMK1VNSH7HYM").await?;

// Transition
let result = client
    .transition(&inst.id, "running", TransitionOptions::default())
    .await?;
println!("Transitioned: {} -> {}", result.from_state, result.to_state);

// Try transition (no error if guard fails)
let result = client
    .try_transition(&inst.id, "done", TransitionOptions::default())
    .await?;

// Transition with options
let opts = TransitionOptions {
    with_data: Some(serde_json::json!({"note": "ready"})),
    memo: Some("Processing complete".to_string()),
    actor: Some("admin".to_string()),
    ..Default::default()
};
client.transition(&inst.id, "done", opts).await?;

// Get trail
let trail = client.trail(&inst.id).await?;
```

### 12.4 — Query Builder

```rust
// Find instances with filters
let results = client.find("Counter")
    .in_state("running")
    .limit(10)
    .execute()
    .await?;

// Aggregate
let stats = client.aggregate("Counter")
    .measure("COUNT()")
    .group_by_state()
    .execute()
    .await?;
```

### 12.5 — WebSocket Subscriptions

```rust
// Subscribe to events
let mut sub = client.subscribe(Some("Counter")).await?;
let event = sub.next_event().await?;
println!("Event: {:?}", event);
```

### 12.6 — Typed API (with Codegen)

```rust
// After running: smql codegen --input machines/ --output src/generated/
let inst = client.spawn_typed::<MyMachine>(data).await?;
let results = client.find_typed::<MyMachine>()
    .in_state("open")
    .execute()
    .await?;
```

---

## 13. Code Generation

The `smql-codegen` crate parses `.smql` files and generates typed Rust code.

```bash
smql codegen --input machines/ --output src/generated/
```

**What gets generated:**

- A Rust struct per machine (with fields matching the DATA block)
- A Rust enum per machine's states
- Type mappings:

| SMQL Type | Rust Type |
|-----------|-----------|
| `TEXT` | `String` |
| `INT` | `i64` |
| `FLOAT` | `f64` |
| `BOOL` | `bool` |
| `UUID` | `String` |
| `DATE` | `String` |
| `DATETIME` | `String` |
| `DURATION` | `String` |
| `MONEY(USD)` | `(i64, String)` |
| `ENUM(a, b)` | Generated Rust enum |
| `REF(Machine)` | `String` |
| `LIST(T)` | `Vec<T>` |
| `SET(T)` | `Vec<T>` |
| `MAP(K, V)` | `std::collections::HashMap<K, V>` |
| `BLOB` | `Vec<u8>` |
| `JSON` | `serde_json::Value` |

---

## 14. Observability

### 14.1 — Prometheus Metrics

SMQL exposes Prometheus-compatible metrics at `GET /metrics`:

```
smql_instances_total{machine="SupportTicket", state="open"} 142
smql_instances_total{machine="SupportTicket", state="in_progress"} 89
smql_transitions_total{machine="SupportTicket", from="open", to="triaged"} 12847
smql_transition_duration_seconds{machine="SupportTicket", quantile="0.95"} 0.003
smql_state_dwell_seconds{machine="SupportTicket", state="in_progress", quantile="0.5"} 14400
smql_guard_failures_total{machine="SupportTicket"} 203
smql_timeout_fires_total{machine="SupportTicket", state="waiting_on_customer"} 47
smql_query_duration_seconds{query_type="find", quantile="0.95"} 0.012
```

Metrics use the `prometheus` crate and are instrumented in the server handlers (the engine itself stays dependency-free).

### 14.2 — Structured Logging

SMQL uses the `tracing` crate for structured, JSON-formatted logging. Each request gets a span with relevant context (machine name, instance ID, transition details).

### 14.3 — Event Streaming

Every transition emits a structured event via the internal EventBus (tokio::broadcast). Events can be consumed via:

- **WebSocket:** connect to `GET /subscribe` (see Section 10.3)
- **Hooks:** `EMIT("event_name")` in transition actions or hooks

```json
{
  "event": "transition",
  "machine": "SupportTicket",
  "instance": "01HWZK4G5C8T3RNMK1VNSH7HYM",
  "from": "in_progress",
  "to": "resolved",
  "actor": "a_emily",
  "timestamp": "2025-03-15T15:00:00.000Z"
}
```

---

## 15. Storage Backends

### 15.1 — Memory Storage (Default)

The default storage backend uses DashMap for concurrent in-memory storage. Fast and suitable for development, testing, and ephemeral workloads.

```bash
smql serve --storage memory
```

### 15.2 — RocksDB Storage (Feature-Gated)

For persistent, production-grade storage, enable the `rocksdb` feature:

```bash
cargo run --bin smql --features rocksdb -- serve --storage rocksdb --data-dir ./data
```

RocksDB uses 6 column families:
- `instances` — instance data
- `state_index` — index by current state
- `machine_index` — index by machine name
- `trails` — transition history
- `parent_index` — parent-child relationships
- `id_index` — ULID-based lookups

Key features:
- Composite keys with NUL (`\x00`) separators
- WriteBatch for atomic multi-write operations
- serde_json serialization

### 15.3 — Timer Persistence

Timeouts are persisted to storage via write-through:
- On register: timer is written to storage immediately
- On cancel: timer is removed from storage
- On startup: `restore_timers()` loads all persisted timers

Timer storage key format: `{instance_id}:{state}` (memory) or `{instance_id}\0{state}` (RocksDB).

---

## 16. Webhook Execution

When a transition fires a `WEBHOOK` action, the SMQL server makes an HTTP POST request:

- **Retry policy:** Retries on 5xx and network errors (exponential backoff). No retry on 4xx.
- **Payload:** JSON body with event data
- **Dry-run fallback:** If the webhook client is not configured, actions are logged but not sent
- **Async:** Webhook execution is fire-and-forget; it does not block the transition

```smql
TRANSITIONS {
  placed -> paid {
    ACTION : WEBHOOK("https://api.example.com/hooks/order-paid")
    -- Or with payload:
    ACTION : WEBHOOK("https://api.example.com/hooks", { order_id: SELF })
  }
}
```

---

## 17. Error Handling

SMQL errors are structured, specific, and actionable. No generic "query failed" messages.

### 17.1 — Transition Errors

```
TransitionDenied {
  instance   : 01HWZK4G5C8T3RNMK1VNSH7HYM
  current    : in_progress
  requested  : resolved
  reason     : GUARD_FAILED
  failures   : [
    {
      guard   : "resolution_note IS SET"
      actual  : resolution_note = NULL
      hint    : "Set resolution_note via WITH clause before transitioning"
    },
    {
      guard   : "ACTOR == assignee OR ACTOR.role == 'admin'"
      actual  : ACTOR = a_bob, assignee = a_emily
      hint    : "Only the assigned agent or an admin can resolve this ticket"
    }
  ]
}
```

All guard failures are collected and reported together (not just the first failure).

### 17.2 — Spawn Errors

```
SpawnRejected {
  machine  : SupportTicket
  reason   : VALIDATION_FAILED
  failures : [
    { field: "customer_id", error: "REQUIRED but not provided" },
    { field: "priority", error: "Value 'urgent' not in ENUM(low, medium, high, critical)" }
  ]
}
```

---

## 18. Expression Reference

### 18.1 — Literals

| Literal | Example |
|---------|---------|
| Text | `"hello world"` (supports `\"`, `\\`, `\n`, `\t` escapes) |
| Integer | `42`, `-5` |
| Float | `3.14`, `-2.71` |
| Boolean | `TRUE`, `FALSE` |
| Null | `NULL` |
| Duration | `60s`, `30m`, `24h`, `7d` |
| Map | `{ key: "value", count: 42 }` |

### 18.2 — Field Access

```smql
-- Simple field
priority

-- Nested (dot notation)
customer.address.city

-- Self reference (current instance)
SELF
SELF.priority

-- Actor reference (who is performing the transition)
ACTOR
ACTOR.role
ACTOR.id
```

### 18.3 — Operators

**Comparison:** `==`, `!=`, `<`, `>`, `<=`, `>=`
**Arithmetic:** `+`, `-`, `*`, `/`
**Logical:** `AND`, `OR`, `NOT`

**Precedence (lowest to highest):**
1. `OR`
2. `AND`
3. `==`, `!=`, `<`, `>`, `<=`, `>=`
4. `+`, `-`
5. `*`, `/`

Parentheses `()` can be used to override precedence.

### 18.4 — Predicates

```smql
-- State checks
STATE IS open
STATE IN { open, triaged, in_progress }

-- Null checks
assignee IS SET           -- field is not null
assignee IS NOT SET       -- field is null
assignee IS NULL          -- alias for IS NOT SET

-- Set membership
priority IN ("high", "critical")

-- Collection predicates
ALL(items, STATE IS confirmed)    -- true for empty (vacuous truth)
ANY(items, STATE IS backordered)  -- false for empty

-- Signal check
SIGNAL FROM PaymentProcess WHERE state == "succeeded"
```

### 18.5 — Built-in Functions

| Function | Returns | Description |
|----------|---------|-------------|
| `elapsed()` | Duration | Time since entering current state |
| `elapsed_in_state()` | Duration | Alias for `elapsed()` |
| `elapsed_since(state)` | Duration | Time since first entry into named state |
| `NOW()` | DateTime | Current timestamp |
| `TODAY()` | Date | Current date |
| `timeout_remaining()` | Duration/Null | Time remaining on active timeout |
| `len(value)` / `length(value)` | Int | String length or collection count |
| `lower(text)` / `lowercase(text)` | Text | Lowercase conversion |
| `upper(text)` / `uppercase(text)` | Text | Uppercase conversion |
| `count(collection)` | Int | Number of elements in collection |

---

## 19. Quick Reference Card

### Comments

```smql
-- This is a line comment
```

### Define

```smql
DEFINE MACHINE Name (
  DATA { field : Type -> constraints }
  STATES { state1, state2 }
  INITIAL STATE state1
  TERMINAL STATES { state2 }
  TRANSITIONS { state1 -> state2 { ... } }
  CHILDREN { child : LIST(Machine) -> MIN(1) }
  PARENT : ParentMachine
  HOOKS { ON SPAWN { ACTION : EMIT("created") } }
  ROLES { admin { CAN SPAWN CAN TRANSITION [state1, state2] CAN QUERY CAN ALTER } }
)
```

### Spawn

```smql
SPAWN Machine { field: value, ... }
SPAWN Machine { ... } THEN TRANSITION TO state
SPAWN BATCH Machine [ { ... }, { ... } ]
```

### Transition

```smql
TRANSITION Machine instance_id TO state
TRANSITION Machine instance_id TO state WITH { field: value }
TRANSITION Machine instance_id TO state MEMO "note" AS "actor"
TRANSITION Machine instance_id THROUGH [state1, state2, state3]
TRANSITION Machine instance_id TO state OR_STAY
TRANSITION Machine instance_id TO state CASCADE
TRY TRANSITION Machine instance_id TO state
TRANSITION ALL Machine WHERE condition TO state AS "actor"
```

### Query

```smql
GET Machine instance_id
FIND Machine WHERE condition SORT BY field ASC LIMIT n OFFSET n
FIND Machine WHERE condition AFTER "cursor_ulid"
TRAIL OF instance_id
AGGREGATE Machine MEASURE COUNT() AS total GROUP BY STATE
PATHS FROM Machine WHERE condition LIMIT n
FUNNEL Machine THROUGH [state1, state2, state3] WHERE condition
COMPARE PATHS Machine SEGMENT BY field WHERE condition
```

### Schema Evolution

```smql
ALTER MACHINE Name
  ADD STATE new_state
  REMOVE STATE old_state MIGRATE TO target
  ADD TRANSITION from -> to { ... }
  REMOVE TRANSITION from -> to
  ADD DATA field : Type -> constraints BACKFILL expr
  REMOVE DATA field
  BACKFILL field = expression
```

---

## Appendix A — Reserved Words

```
DEFINE  MACHINE  STATES  INITIAL  TERMINAL  DATA  TRANSITIONS  TRANSITION
GUARD  ACTION  MUTATE  TIMEOUT  MEMO  SPAWN  TRY  THROUGH  CASCADE  BATCH
FIND  GET  WHERE  SORT  LIMIT  OFFSET  AFTER  STATE
TRAIL  PATHS  FUNNEL  AGGREGATE  COMPARE  SEGMENT
MEASURE  GROUP  BY  OF  FROM  TO  AS  WITH
AVG  COUNT  SUM  MIN  MAX  PERCENTILE
EMIT  NOTIFY  LOG  WEBHOOK  SIGNAL
SELF  ACTOR  ANY  ALL  EXCEPT  IN  IS  SET  NULL
OR  AND  NOT  OR_STAY
ALTER  ADD  REMOVE  BACKFILL  MIGRATE
HOOKS  BEFORE  AFTER  EACH  ON  ENTER  EXIT  DWELL  SUBSCRIBE  DELIVER
PARENT  CHILDREN  REF  LIST  MAP  ENUM  REQUIRED  OPTIONAL
DEFAULT  RANGE  PATTERN  UNIQUE  ROLES
MONEY  UUID  TEXT  INT  FLOAT  BOOL  DATE  DATETIME  DURATION  BLOB  JSON
TRUE  FALSE  NOW  TODAY  ASC  DESC  DELETED
STUCK_IN  TIMEOUT_REMAINING  HAS_VISITED  NEVER_VISITED  ALIVE  TERMINATED
CONTAINS
```

---

## Appendix B — Grammar (Simplified EBNF)

```ebnf
statement       ::= machine_def | spawn_stmt | transition_stmt | query_stmt | alter_stmt

machine_def     ::= 'DEFINE' 'MACHINE' IDENT '(' machine_body ')'
machine_body    ::= (data_block | states_block | initial_block |
                     terminal_block | transitions_block | children_block |
                     hooks_block | roles_block | parent_block)*

states_block    ::= 'STATES' '{' IDENT (',' IDENT)* '}'
initial_block   ::= 'INITIAL' 'STATE' IDENT
terminal_block  ::= 'TERMINAL' 'STATES' '{' IDENT (',' IDENT)* '}'
data_block      ::= 'DATA' '{' field_def (',' field_def)* '}'
field_def       ::= IDENT ':' type_def ('->' constraint (',' constraint)*)?
parent_block    ::= 'PARENT' ':' IDENT

transition_def  ::= source '->' target '{' transition_clause* '}'
source          ::= IDENT | 'ANY' ('EXCEPT' 'FROM' '{' IDENT (',' IDENT)* '}')?
target          ::= IDENT
transition_clause ::= guard | action | mutate | timeout | signal_parent
                    | except_from
guard           ::= 'GUARD' ':' expression
action          ::= 'ACTION' ':' action_expr
mutate          ::= 'MUTATE' ':' IDENT '=' expression
timeout         ::= 'TIMEOUT' ':' DURATION '->' IDENT
signal_parent   ::= 'SIGNAL' 'PARENT' 'TO' IDENT

spawn_stmt      ::= 'SPAWN' IDENT '{' data_fields '}'
                     ('THEN' 'TRANSITION' 'TO' IDENT)?
                   | 'SPAWN' 'BATCH' IDENT '[' '{' data_fields '}' (',' '{' data_fields '}')* ']'

transition_stmt ::= ('TRY')? 'TRANSITION' IDENT IDENT 'TO' IDENT
                     ('WITH' '{' data_fields '}')?
                     ('MEMO' STRING)?
                     ('AS' (STRING | IDENT))?
                     ('THROUGH' '[' IDENT (',' IDENT)* ']')?
                     ('OR_STAY')?
                     ('CASCADE')?
                   | 'TRANSITION' 'ALL' IDENT 'WHERE' expression 'TO' IDENT
                     ('WITH' '{' data_fields '}')?
                     ('MEMO' STRING)?
                     ('AS' (STRING | IDENT))?

query_stmt      ::= get_stmt | find_stmt | trail_stmt | aggregate_stmt
                   | paths_stmt | funnel_stmt | compare_stmt
get_stmt        ::= 'GET' IDENT IDENT
find_stmt       ::= 'FIND' IDENT
                     ('WHERE' expression)?
                     ('SORT' 'BY' sort_spec (',' sort_spec)*)?
                     ('LIMIT' INT)?
                     ('OFFSET' INT)?
                     ('AFTER' STRING)?
trail_stmt      ::= 'TRAIL' 'OF' IDENT
aggregate_stmt  ::= 'AGGREGATE' IDENT
                     'MEASURE' measure_spec (',' measure_spec)*
                     ('WHERE' expression)?
                     ('GROUP' 'BY' group_spec (',' group_spec)*)
paths_stmt      ::= 'PATHS' 'FROM' IDENT ('WHERE' expression)? ('LIMIT' INT)?
funnel_stmt     ::= 'FUNNEL' IDENT 'THROUGH' '[' IDENT (',' IDENT)* ']'
                     ('WHERE' expression)?
compare_stmt    ::= 'COMPARE' 'PATHS' IDENT 'SEGMENT' 'BY' IDENT
                     ('WHERE' expression)?

alter_stmt      ::= 'ALTER' 'MACHINE' IDENT alter_op+
alter_op        ::= 'ADD' 'STATE' IDENT
                   | 'REMOVE' 'STATE' IDENT 'MIGRATE' 'TO' IDENT
                   | 'ADD' 'TRANSITION' transition_def
                   | 'REMOVE' 'TRANSITION' IDENT '->' IDENT
                   | 'ADD' 'DATA' field_def ('BACKFILL' expression)?
                   | 'REMOVE' 'DATA' IDENT
                   | 'BACKFILL' IDENT '=' expression

expression      ::= or_expr
or_expr         ::= and_expr ('OR' and_expr)*
and_expr        ::= comparison ('AND' comparison)*
comparison      ::= addition (comp_op addition)?
addition        ::= multiplication (('+' | '-') multiplication)*
multiplication  ::= unary (('*' | '/') unary)*
unary           ::= 'NOT' unary | '-' unary | primary
primary         ::= literal | IDENT ('.' IDENT)* | function_call
                   | state_pred | null_check | set_membership
                   | '(' expression ')'
```

---

## Appendix C — Planned Features

The following features are defined in the SMQL specification but not yet implemented in the engine. They are reserved for future releases:

| Feature | Description |
|---------|-------------|
| `GROUP state_group { ... }` | Named state groups for transitions |
| `DEFINE VIEW` | Flattened query views for dashboards |
| `DEFINE PROJECTION` | Materialized, auto-refreshing projections |
| `SELECT` clause in FIND | Select specific fields in query results |
| `MODIFY TRANSITION` in ALTER | Modify guards/actions on existing transitions |
| `ON DWELL(state, > duration)` hooks | Fire actions after dwelling in a state |
| `STUCK_IN(state, > duration)` predicate | Query for stuck instances |
| `TIMEOUT_REMAINING` predicate | Query instances approaching timeout |
| `HAS_VISITED(state)` predicate | Instances that have visited a specific state |
| `NEVER_VISITED(state)` predicate | Instances that have never visited a state |
| `ALIVE` / `TERMINATED` predicates | Filter by lifecycle status |
| `TRAIL CONTAINS (pattern)` | Sequential pattern matching on trail |
| `SUBSCRIBE ... DELIVER TO WEBHOOK` | Declarative event subscriptions in SMQL |
| `transition_time(state_a, state_b)` | Measure time between two states |
| `duration_in(state)` | Total time spent in a state |
| `total_lifecycle_duration()` | Total instance lifetime |
| `entered_state_at()` | Timestamp of state entry |
| `BETWEEN` operator | Range operator for dates/numbers |
| Data-level access control | Per-field read/write permissions by role |
| Extended role syntax (`EXTENDS`, `CAN ALL`) | Role inheritance and full access |
| RocksDB compaction/TTL | Automatic cleanup of old trail entries |

---

*SMQL Language Specification — v0.2.0*
*Designed for developers who believe data has a lifecycle.*
