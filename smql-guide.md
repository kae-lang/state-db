# SMQL — State Machine Query Language

## The Complete Developer Guide

**Version 0.1.0 — Language Specification & Developer Reference**

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

**Machine** — A blueprint that defines a lifecycle. Think of it as the equivalent of a table in SQL, but instead of defining columns, you define states and the rules governing movement between them. Example: `OrderMachine`, `ClaimMachine`, `TicketMachine`.

**Instance** — A single entity living within a machine. If `OrderMachine` is the blueprint, then `Order/8832` is an instance. Each instance has a current state, associated data, and a full transition history. Equivalent to a row in SQL.

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
| `BOOL` | Boolean | `true` |
| `UUID` | Universally unique ID | `a1b2c3d4-...` |
| `DATE` | Calendar date | `2025-03-15` |
| `DATETIME` | Timestamp with timezone | `2025-03-15T14:30:00Z` |
| `DURATION` | Time span | `48h`, `30m`, `7d` |
| `ENUM(...)` | Constrained set of values | `ENUM(low, medium, high)` |
| `REF(Machine)` | Reference to another instance | `REF(Customer)` |
| `LIST(Type)` | Ordered collection | `LIST(TEXT)` |
| `SET(Type)` | Unique collection | `SET(UUID)` |
| `MAP(K, V)` | Key-value pairs | `MAP(TEXT, INT)` |
| `BLOB` | Binary large object | File uploads |
| `MONEY` | Decimal with currency | `MONEY(USD)` |
| `JSON` | Flexible structure | Schema-free nested data |

**Data constraints:**

```smql
-- Constraints are chained with commas
customer_id : UUID -> REQUIRED
email       : TEXT -> REQUIRED, PATTERN(/^.+@.+\..+$/), UNIQUE
age         : INT  -> RANGE(0, 150)
score       : FLOAT -> MIN(0.0)
metadata    : JSON -> OPTIONAL, MAX_DEPTH(5)
```

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
      GUARD   : priority != low OR elapsed() < 24h
      ACTION  : NOTIFY(assignee, "ticket.assigned")
      ACTION  : LOG("Ticket triaged by {ACTOR}")
    }

    triaged -> in_progress {
      GUARD   : ACTOR == assignee OR ACTOR.role == "admin"
      ACTION  : NOTIFY(customer_id, "ticket.in_progress")
    }

    in_progress -> waiting_on_customer {
      GUARD   : ACTOR == assignee
      TIMEOUT : 72h -> auto_resolve
      ACTION  : NOTIFY(customer_id, "ticket.needs_response")
    }

    waiting_on_customer -> in_progress {
      -- Customer responded, back to active work
      GUARD   : ACTOR.id == customer_id OR ACTOR == assignee
    }

    in_progress -> resolved {
      GUARD   : resolution_note IS SET
      GUARD   : ACTOR == assignee OR ACTOR.role == "admin"
      ACTION  : NOTIFY(customer_id, "ticket.resolved")
      TIMEOUT : 7d -> auto_close
    }

    resolved -> reopened {
      GUARD   : ACTOR.id == customer_id
      GUARD   : elapsed_since(resolved) < 30d
      ACTION  : NOTIFY(assignee, "ticket.reopened")
    }

    reopened -> in_progress {
      GUARD   : assignee IS SET
    }

    resolved -> closed {
      GUARD   : elapsed_since(resolved) >= 7d OR ACTOR.role == "admin"
      PROMPT  : satisfaction -> "How would you rate your experience? (1-5)"
      ACTION  : NOTIFY(customer_id, "ticket.closed")
    }

    -- Wildcard: any non-terminal state can be escalated
    ANY -> triaged {
      EXCEPT FROM { open, closed }
      GUARD   : ACTOR.role IN ("admin", "supervisor")
      MUTATE  : priority = critical
      ACTION  : NOTIFY(assignee, "ticket.escalated")
      ACTION  : LOG("Escalated by {ACTOR}")
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
  GUARD : <boolean expression>

  -- MUTATE: data modifications that happen atomically with the transition.
  -- These are guaranteed changes — not suggestions.
  MUTATE : <field> = <expression>

  -- ACTION: side effects triggered after a successful transition.
  -- Actions are asynchronous and non-blocking by default.
  -- A failed action does NOT roll back the transition.
  ACTION : <effect>

  -- TIMEOUT: automatic transition if the instance stays in the
  -- target state for too long. Only one timeout per target state.
  TIMEOUT : <duration> -> <target_state>

  -- PROMPT: request additional data from the actor during this transition.
  -- The runtime collects this before completing the transition.
  PROMPT : <field> -> <message>

  -- MEMO: attach a human-readable note to this transition in the trail.
  -- Useful for audit purposes.
  MEMO : OPTIONAL TEXT

  -- RESTRICT: limit who can perform this transition.
  RESTRICT : ROLE IN ("admin", "agent")
}
```

### 3.5 — Wildcard and Group Transitions

For transitions that apply from many states:

```smql
-- From any state (except those excluded)
ANY -> cancelled {
  EXCEPT FROM { shipped, delivered, closed }
  GUARD  : ACTOR.role == "admin" OR ACTOR.id == customer_id
  ACTION : release_inventory()
}

-- From a named group of states
GROUP active_states { triaged, in_progress, waiting_on_customer }

active_states -> on_hold {
  GUARD  : ACTOR.role IN ("admin", "supervisor")
  MUTATE : hold_reason = REQUIRED TEXT
  TIMEOUT: 14d -> triaged
}
```

---

## 4. Creating and Transitioning Instances

### 4.1 — Spawning an Instance

```smql
-- Basic creation. Instance enters the INITIAL STATE automatically.
SPAWN SupportTicket {
  customer_id : "c_9f83a1b2"
  subject     : "Cannot access dashboard"
  description : "Getting 403 error since this morning"
  priority    : high
}
-- Returns: SupportTicket/tk_00123 IN STATE open
```

```smql
-- Spawn and immediately transition (if guards allow)
SPAWN SupportTicket {
  customer_id : "c_9f83a1b2"
  subject     : "Billing question"
  assignee    : Agent/"a_emily"
} THEN TRANSITION TO triaged
-- Returns: SupportTicket/tk_00124 IN STATE triaged
```

```smql
-- Batch spawn
SPAWN BATCH SupportTicket FROM [
  { customer_id: "c_001", subject: "Issue A", priority: low },
  { customer_id: "c_002", subject: "Issue B", priority: high },
  { customer_id: "c_003", subject: "Issue C", priority: medium }
]
-- Returns: 3 instances created in STATE open
```

### 4.2 — Performing Transitions

The core write operation in SMQL is `TRANSITION`, not `UPDATE`:

```smql
-- Basic transition
TRANSITION SupportTicket/tk_00123 TO triaged
  WITH { assignee: Agent/"a_emily" }
  MEMO "Assigned to Emily — she handled a similar issue last week"
  AS Agent/"a_supervisor"
```

Let's break this down:

- `TRANSITION ... TO` — the target state. The database checks that a valid path exists from the current state.
- `WITH { }` — data mutations bundled with the transition. These are applied atomically.
- `MEMO` — an optional human-readable note stored in the trail.
- `AS` — the actor performing the transition. Used for guard evaluation and audit.

**What happens internally:**

1. Database locks the instance.
2. Verifies current state → target state is a declared transition.
3. Applies `WITH` mutations to the instance data.
4. Evaluates all `GUARD` conditions. If any fail → reject with error, rollback mutations.
5. Applies any `MUTATE` clauses from the transition definition.
6. Records the transition in the trail (from_state, to_state, actor, timestamp, memo, data_snapshot).
7. Fires `ACTION` side effects asynchronously.
8. Releases lock.

**The entire sequence (steps 1–6) is atomic.** Actions (step 7) are eventual.

### 4.3 — Conditional Transitions

```smql
-- Only transition if guards pass; otherwise do nothing (no error)
TRY TRANSITION SupportTicket/tk_00123 TO resolved
  WITH { resolution_note: "Cleared browser cache" }
  AS Agent/"a_emily"
```

```smql
-- Transition with fallback
TRANSITION SupportTicket/tk_00123 TO resolved
  WITH { resolution_note: "Fixed" }
  OR STAY WITH { last_attempt: NOW() }
  AS Agent/"a_emily"
-- If transition fails, update data but remain in current state
```

### 4.4 — Batch Transitions

```smql
-- Transition all matching instances
TRANSITION ALL SupportTicket
  WHERE STATE IS resolved
    AND elapsed_in_state() > 7d
  TO closed
  AS System/"auto_closer"
```

### 4.5 — Multi-Step Transitions (Choreography)

Sometimes you need an instance to move through multiple states in one logical operation:

```smql
-- Express checkout: paid → fulfilled → shipped in one call
-- Each intermediate transition's guards are still evaluated
TRANSITION Order/ord_555 THROUGH [paid, fulfilled, shipped]
  WITH {
    payment: { verified: true, method: "card" },
    shipment: { tracking: "TRK_998877" }
  }
  AS System/"express_checkout"
```

The database evaluates each hop sequentially. If any intermediate guard fails, it stops at the last successful state and reports where it halted.

---

## 5. Querying

### 5.1 — Basic Queries

```smql
-- Find by ID
GET SupportTicket/tk_00123

-- Find by current state
FIND SupportTicket WHERE STATE IS open

-- Compound filters
FIND SupportTicket
  WHERE STATE IS in_progress
    AND priority == critical
    AND assignee == Agent/"a_emily"
  SORT BY spawned_at DESC
  LIMIT 20
```

### 5.2 — State-Aware Queries

These are queries that only make sense in a state machine context — and they're native to SMQL:

```smql
-- Stuck instances: in a state longer than expected
FIND SupportTicket WHERE STUCK_IN(triaged, > 4h)

-- Instances approaching timeout
FIND SupportTicket WHERE TIMEOUT_REMAINING < 1h

-- Instances that have ever visited a specific state
FIND SupportTicket WHERE HAS_VISITED(waiting_on_customer)

-- Instances that have NEVER visited a state
FIND SupportTicket WHERE NEVER_VISITED(escalated)

-- Instances currently in any of a set of states
FIND SupportTicket WHERE STATE IN { open, triaged, in_progress }

-- Instances in non-terminal states (still "alive")
FIND SupportTicket WHERE ALIVE

-- Instances in terminal states
FIND SupportTicket WHERE TERMINATED
```

### 5.3 — Temporal Queries

Time is a first-class dimension. Every state entry and exit is recorded automatically.

```smql
-- How long has this instance been in its current state?
GET SupportTicket/tk_00123 -> elapsed_in_state()

-- When did this instance enter its current state?
GET SupportTicket/tk_00123 -> entered_state_at()

-- How long did it spend in a specific past state?
GET SupportTicket/tk_00123 -> duration_in(triaged)

-- Total time from spawn to terminal state
FIND SupportTicket
  WHERE TERMINATED
  SELECT id, total_lifecycle_duration()
  SORT BY total_lifecycle_duration() DESC
  LIMIT 10
-- "Show me the 10 tickets that took the longest to close"

-- Average time between two states
AGGREGATE SupportTicket
  WHERE TERMINATED AND spawned_at > 2025-01-01
  MEASURE AVG(transition_time(open, resolved)) AS avg_resolution
  GROUP BY priority
-- "Average resolution time by priority this year"
```

### 5.4 — Trail Queries

The trail is the immutable history. You can query it directly:

```smql
-- Full trail for an instance
TRAIL OF SupportTicket/tk_00123

-- Returns:
-- | step | from        | to                  | actor         | at                   | memo              |
-- |------|-------------|---------------------|---------------|----------------------|-------------------|
-- | 1    | (spawn)     | open                | Customer/c_01 | 2025-03-01T09:00:00Z |                   |
-- | 2    | open        | triaged             | Agent/a_sup   | 2025-03-01T09:15:00Z | "Assigned Emily"  |
-- | 3    | triaged     | in_progress         | Agent/a_emily | 2025-03-01T09:20:00Z |                   |
-- | 4    | in_progress | waiting_on_customer | Agent/a_emily | 2025-03-01T10:00:00Z |                   |
-- | 5    | waiting...  | in_progress         | Customer/c_01 | 2025-03-01T14:30:00Z |                   |
-- | 6    | in_progress | resolved            | Agent/a_emily | 2025-03-01T15:00:00Z | "Cache cleared"   |


-- Trail queries across instances
FIND SupportTicket
  WHERE TRAIL CONTAINS (in_progress -> waiting_on_customer -> in_progress)
  AND TRAIL.count(waiting_on_customer) >= 3
-- "Tickets that bounced back to the customer 3+ times"

-- Who performed the most transitions today?
AGGREGATE TRAILS OF SupportTicket
  WHERE TRAIL.at > TODAY()
  MEASURE COUNT(*) AS transitions
  GROUP BY TRAIL.actor
  SORT BY transitions DESC
```

### 5.5 — Path Analysis

Analyzing the routes instances take through their lifecycle:

```smql
-- Most common paths through the machine
PATHS OF SupportTicket
  WHERE spawned_at > 2025-01-01 AND TERMINATED
  LIMIT 5

-- Returns:
-- | path                                              | count | pct   | avg_duration |
-- |---------------------------------------------------|-------|-------|--------------|
-- | open → triaged → in_progress → resolved → closed  | 4521  | 62.1% | 4.2h         |
-- | open → triaged → in_progress → w_o_c → ... → closed | 1203 | 16.5% | 18.7h       |
-- | open → cancelled                                  | 892   | 12.2% | 0.3h         |
-- | open → triaged → in_progress → escalated → ...    | 441   | 6.1%  | 32.1h        |
-- | ...                                               |       |       |              |


-- Path comparison: how do critical tickets flow vs low priority?
COMPARE PATHS OF SupportTicket
  SEGMENT BY priority
  WHERE TERMINATED AND spawned_at > 2025-01-01
```

### 5.6 — Aggregations

```smql
-- State distribution: how many instances in each state right now?
AGGREGATE SupportTicket
  MEASURE COUNT(*) AS total
  GROUP BY STATE

-- Funnel analysis: conversion between states
FUNNEL SupportTicket
  THROUGH [open, triaged, in_progress, resolved, closed]
  WHERE spawned_at BETWEEN 2025-03-01 AND 2025-03-31
-- Returns drop-off rates at each stage

-- Throughput: transitions per hour
AGGREGATE TRAILS OF SupportTicket
  WHERE TRAIL.at > NOW() - 24h
  MEASURE COUNT(*) AS transitions
  GROUP BY HOUR(TRAIL.at)
```

---

## 6. Machine Composition

### 6.1 — Child Machines

Machines can own other machines, creating parent-child lifecycles:

```smql
DEFINE MACHINE Order (

  DATA {
    customer : REF(Customer)
    total    : MONEY(USD)
  }

  STATES { draft, placed, paid, fulfilled, shipped, delivered }
  INITIAL STATE draft
  TERMINAL STATES { delivered, cancelled }

  CHILDREN {
    -- An order has line items and a shipment
    items    : LIST(LineItem)    -> MIN(1)
    shipment : OPTIONAL(Shipment)
  }

  TRANSITIONS {
    paid -> fulfilled {
      -- Guard references children's states
      GUARD : ALL(items, STATE IS confirmed)
      MUTATE : shipment = SPAWN Shipment { order: SELF }
    }

    fulfilled -> shipped {
      -- Wait for child machine to reach a state
      GUARD : shipment.STATE IS dispatched
    }

    shipped -> delivered {
      GUARD : shipment.STATE IS delivered
    }
  }
)

DEFINE MACHINE LineItem (
  PARENT : Order
  DATA {
    product  : REF(Product)
    quantity : INT -> MIN(1)
    price    : MONEY(USD)
  }
  STATES { pending, confirmed, backordered, cancelled }
  INITIAL STATE pending
)

DEFINE MACHINE Shipment (
  PARENT : Order
  DATA {
    tracking : TEXT -> OPTIONAL
    carrier  : ENUM(fedex, ups, dhl, usps)
  }
  STATES { created, dispatched, in_transit, delivered, lost }
  INITIAL STATE created

  TRANSITIONS {
    delivered -> (SIGNAL PARENT TO delivered)
    -- When shipment is delivered, signal the parent Order
  }
)
```

### 6.2 — Machine References (Cross-Machine Queries)

```smql
-- Find all orders where ANY line item is backordered
FIND Order
  WHERE ANY(items, STATE IS backordered)

-- Find all shipments whose parent order is from a specific customer
FIND Shipment
  WHERE PARENT(Order).customer == Customer/"c_001"

-- Cascade: cancel an order and all its children
TRANSITION Order/ord_001 TO cancelled CASCADE
-- All LineItems → cancelled, Shipment → cancelled (if applicable)
```

### 6.3 — Signals Between Machines

Unrelated machines can communicate through signals:

```smql
DEFINE MACHINE PaymentProcess (
  STATES { initiated, processing, succeeded, failed }

  TRANSITIONS {
    processing -> succeeded {
      SIGNAL Order WHERE Order.payment_ref == SELF.id
        TO TRANSITION TO paid
    }
    processing -> failed {
      SIGNAL Order WHERE Order.payment_ref == SELF.id
        TO TRANSITION TO payment_failed
    }
  }
)
```

---

## 7. Hooks and Side Effects

### 7.1 — Action Types

Actions are side effects triggered by transitions. They execute asynchronously after the transition commits.

```smql
TRANSITIONS {
  open -> triaged {
    -- Built-in actions
    ACTION : NOTIFY(assignee, "ticket.assigned", { ticket: SELF })
    ACTION : LOG("Triaged: {SELF.id} assigned to {assignee}")
    ACTION : EMIT("ticket.triaged", { ticket: SELF, actor: ACTOR })

    -- Webhook action
    ACTION : WEBHOOK("https://api.example.com/hooks/ticket-triaged", {
      method: POST,
      body: { ticket_id: SELF.id, assignee: assignee },
      retry: 3,
      timeout: 5s
    })

    -- Spawn another machine instance
    ACTION : SPAWN Notification {
      recipient: assignee,
      message: "You've been assigned ticket {SELF.id}"
    }
  }
}
```

### 7.2 — Global Hooks

Apply logic to all transitions within a machine:

```smql
DEFINE MACHINE SupportTicket (
  -- ...

  HOOKS {
    -- Runs before every transition (can reject)
    BEFORE EACH TRANSITION {
      GUARD : ACTOR IS AUTHENTICATED
      LOG("Transition attempted: {FROM} -> {TO} by {ACTOR}")
    }

    -- Runs after every transition (cannot reject, informational)
    AFTER EACH TRANSITION {
      EMIT("{MACHINE}.{TO}", { instance: SELF, actor: ACTOR })
      METRIC("transition_count", INCREMENT, tags: { machine: MACHINE, to: TO })
    }

    -- Runs when any instance enters a specific state
    ON ENTER critical {
      NOTIFY(Channel/"ops-alerts", "Critical ticket: {SELF.id}")
    }

    -- Runs when any instance has been in a state too long
    ON DWELL(in_progress, > 8h) {
      MUTATE : priority = UPGRADE(priority)
      NOTIFY(SELF.assignee, "Ticket {SELF.id} needs attention — 8h in progress")
    }
  }
)
```

---

## 8. Views and Projections

Sometimes you need to see your state machine data in a traditional tabular format — for dashboards, reports, or integration with existing tools.

### 8.1 — Defining Views

```smql
-- A flattened view for a dashboard
DEFINE VIEW TicketDashboard AS
  FIND SupportTicket
  SELECT
    id,
    subject,
    STATE AS status,
    priority,
    assignee.name AS agent_name,
    spawned_at AS created,
    elapsed_in_state() AS time_in_current_state,
    TRAIL.count(*) AS total_transitions,
    TRAIL.count(waiting_on_customer) AS times_waiting
  WHERE ALIVE
  SORT BY priority DESC, spawned_at ASC
```

### 8.2 — Materialized Projections

For analytics and BI tools that need a traditional table shape:

```smql
-- Continuously maintained projection into a relational shape
DEFINE PROJECTION TicketMetrics
  MATERIALIZED REFRESH EVERY 5m
  AS AGGREGATE SupportTicket
    WHERE spawned_at > NOW() - 90d
    MEASURE
      COUNT(*) AS total,
      COUNT(WHERE STATE IS open) AS open_count,
      AVG(transition_time(open, resolved)) AS avg_resolution_time,
      PERCENTILE(95, transition_time(open, resolved)) AS p95_resolution,
      COUNT(WHERE HAS_VISITED(escalated)) AS escalated_count
    GROUP BY DATE(spawned_at), priority
```

---

## 9. Schema Evolution

### 9.1 — Adding States

```smql
ALTER MACHINE SupportTicket
  ADD STATE on_hold BETWEEN triaged AND in_progress
  ADD TRANSITION in_progress -> on_hold {
    GUARD  : ACTOR.role == "admin"
    TIMEOUT: 7d -> triaged
  }
  ADD TRANSITION on_hold -> in_progress {
    GUARD  : ACTOR.role == "admin"
  }
```

### 9.2 — Removing States

You can't just delete a state if instances are in it. SMQL requires a migration:

```smql
ALTER MACHINE SupportTicket
  REMOVE STATE on_hold
  MIGRATE INSTANCES IN on_hold TO triaged
  MEMO "Removing on_hold state — all held tickets return to triage"
```

### 9.3 — Modifying Transitions

```smql
ALTER MACHINE SupportTicket
  MODIFY TRANSITION in_progress -> resolved {
    ADD GUARD : satisfaction IS SET  -- now require satisfaction before resolving
  }

ALTER MACHINE SupportTicket
  REMOVE TRANSITION resolved -> reopened
  -- Customers can no longer reopen resolved tickets
```

### 9.4 — Data Migrations

```smql
ALTER MACHINE SupportTicket
  ADD DATA { sla_tier : ENUM(standard, premium, enterprise) -> DEFAULT(standard) }
  BACKFILL sla_tier FROM (
    CASE
      WHEN customer.plan == "enterprise" THEN enterprise
      WHEN customer.plan == "pro" THEN premium
      ELSE standard
    END
  )
```

---

## 10. Access Control

### 10.1 — Role-Based Transition Control

```smql
DEFINE ROLES FOR SupportTicket {
  customer {
    CAN SPAWN
    CAN TRANSITION: waiting_on_customer -> in_progress
    CAN TRANSITION: resolved -> reopened
    CAN VIEW: own instances (WHERE customer_id == ACTOR.id)
  }

  agent {
    CAN TRANSITION: open -> triaged, triaged -> in_progress,
                    in_progress -> waiting_on_customer,
                    in_progress -> resolved
    CAN VIEW: assigned instances (WHERE assignee == ACTOR)
    CAN VIEW: unassigned instances (WHERE assignee IS NOT SET)
  }

  supervisor {
    EXTENDS agent
    CAN TRANSITION: ANY -> triaged  -- escalation
    CAN TRANSITION: resolved -> closed
    CAN VIEW: all instances
  }

  admin {
    CAN ALL
  }
}
```

### 10.2 — Data-Level Permissions

```smql
DEFINE DATA ACCESS FOR SupportTicket {
  customer {
    CAN READ: subject, description, priority, STATE
    CAN WRITE ON SPAWN: subject, description, priority
    CANNOT READ: internal_notes, assignee_notes
  }

  agent {
    CAN READ: ALL
    CAN WRITE: assignee, priority, tags, resolution_note
  }
}
```

---

## 11. Developer Experience — SDKs and Tooling

### 11.1 — TypeScript SDK

SMQL ships with first-class TypeScript support. Machine definitions generate types automatically:

```typescript
import { connect, Machine } from '@smql/client';

const db = await connect('smql://localhost:5432/myapp');

// Types are generated from your machine definitions
// via: smql codegen --lang typescript --out ./types

import type { SupportTicket, SupportTicketState } from './types';

// Spawn
const ticket = await db.machine('SupportTicket').spawn({
  customer_id: 'c_9f83a1b2',
  subject: 'Cannot access dashboard',
  description: 'Getting 403 error since this morning',
  priority: 'high',
});

console.log(ticket.id);     // "tk_00123"
console.log(ticket.state);  // "open"

// Transition — fully typed, autocomplete on valid target states
await ticket.transition('triaged', {
  with: { assignee: 'a_emily' },
  memo: 'Assigned to Emily',
  as: 'a_supervisor',
});

// This would be a compile-time error:
// await ticket.transition('delivered')
//                          ^^^^^^^^^ Error: no valid transition
//                          from "triaged" to "delivered"

// Query
const stuck = await db.machine('SupportTicket').find({
  where: {
    stuckIn: ['triaged', { moreThan: '4h' }],
    priority: 'critical',
  },
  limit: 10,
});

// Trail
const trail = await ticket.trail();
trail.forEach(step => {
  console.log(`${step.from} → ${step.to} at ${step.at} by ${step.actor}`);
});
```

### 11.2 — Python SDK

```python
from smql import connect

db = connect("smql://localhost:5432/myapp")

# Spawn
ticket = db.SupportTicket.spawn(
    customer_id="c_9f83a1b2",
    subject="Cannot access dashboard",
    description="Getting 403 error since this morning",
    priority="high",
)

# Transition
ticket.transition(
    to="triaged",
    with_data={"assignee": "a_emily"},
    memo="Assigned to Emily",
    actor="a_supervisor",
)

# Query with state-aware filters
stuck_tickets = db.SupportTicket.find(
    stuck_in=("triaged", ">4h"),
    priority="critical",
    limit=10,
)

# Path analysis
paths = db.SupportTicket.paths(
    where={"terminated": True, "spawned_at__gt": "2025-01-01"},
    limit=5,
)
for path in paths:
    print(f"{' → '.join(path.states)}  ({path.count} instances, avg {path.avg_duration})")

# Aggregations
metrics = db.SupportTicket.aggregate(
    measure={"avg_resolution": "AVG(transition_time(open, resolved))"},
    group_by="priority",
)
```

### 11.3 — CLI

```bash
# Connect to a database
$ smql connect myapp

# Apply machine definitions
$ smql apply ./machines/support_ticket.smql
✓ Machine SupportTicket created (9 states, 12 transitions)

# Interactive REPL
$ smql repl
smql> FIND SupportTicket WHERE STUCK_IN(triaged, > 4h)
┌────────────┬─────────────────────┬──────────┬─────────────┐
│ id         │ subject             │ priority │ stuck_for   │
├────────────┼─────────────────────┼──────────┼─────────────┤
│ tk_00091   │ Login broken        │ critical │ 6h 12m      │
│ tk_00145   │ Billing error       │ high     │ 4h 45m      │
│ tk_00203   │ API timeout         │ high     │ 4h 03m      │
└────────────┴─────────────────────┴──────────┴─────────────┘
3 results (12ms)

# Visualize a machine
$ smql visualize SupportTicket --format png
✓ Saved to support_ticket_states.png

# Diff between machine versions
$ smql diff SupportTicket@v3 SupportTicket@v4
+ Added state: on_hold
+ Added transition: in_progress -> on_hold
+ Added transition: on_hold -> in_progress
~ Modified transition: in_progress -> resolved
  + Added guard: satisfaction IS SET

# Dry-run a transition (validates without committing)
$ smql dry-run TRANSITION SupportTicket/tk_00123 TO resolved
✗ Guard failed: resolution_note IS SET (resolution_note is NULL)
✗ Guard failed: ACTOR == assignee (ACTOR is "a_bob", assignee is "a_emily")
Transition would be REJECTED (2 guard failures)

# Export trail as JSON (for external audit systems)
$ smql trail SupportTicket/tk_00123 --format json > trail.json
```

### 11.4 — Visual Studio Code Extension

The SMQL VS Code extension provides:

- Syntax highlighting for `.smql` files.
- Autocomplete for state names, transition targets, and data fields.
- Inline validation — red squiggles under invalid transitions (e.g., referencing an undeclared state).
- **Live machine diagram**: a sidebar panel that renders your machine as a state diagram in real time as you type, highlighting states, transitions, guards, and timeouts.
- **Guard evaluation preview**: hover over a guard to see example data that would pass or fail it.
- **Transition simulator**: right-click an instance ID to step through transitions interactively, watching guards evaluate in real time.

### 11.5 — Dashboard (Built-in Web UI)

Every SMQL database ships with a built-in operational dashboard at `http://localhost:5432/_ui`:

- **State map**: visual diagram of each machine with real-time counts per state (pulsing nodes for states with active timeouts).
- **Instance inspector**: click any instance to see its current data, full trail, and available transitions.
- **Stuck alerts**: configurable alerts for instances dwelling in a state beyond thresholds.
- **Throughput monitor**: transitions per second, broken down by machine and transition type.
- **Path explorer**: interactive Sankey diagram showing how instances flow through states.
- **Time-travel**: slide a time scrubber to see the state distribution at any historical point.

---

## 12. Observability

### 12.1 — Built-in Metrics

SMQL exposes Prometheus-compatible metrics out of the box:

```
smql_instances_total{machine="SupportTicket", state="open"} 142
smql_instances_total{machine="SupportTicket", state="in_progress"} 89
smql_transitions_total{machine="SupportTicket", from="open", to="triaged"} 12847
smql_transition_duration_seconds{machine="SupportTicket", transition="open_to_triaged", quantile="0.95"} 0.003
smql_state_dwell_seconds{machine="SupportTicket", state="in_progress", quantile="0.5"} 14400
smql_guard_failures_total{machine="SupportTicket", transition="in_progress_to_resolved", guard="resolution_note_set"} 203
smql_timeout_fires_total{machine="SupportTicket", state="waiting_on_customer"} 47
```

### 12.2 — Event Stream

Every transition emits a structured event that can be consumed externally:

```json
{
  "event": "transition",
  "machine": "SupportTicket",
  "instance": "tk_00123",
  "from": "in_progress",
  "to": "resolved",
  "actor": "Agent/a_emily",
  "timestamp": "2025-03-15T15:00:00.000Z",
  "guards_evaluated": 2,
  "guards_passed": 2,
  "mutations": { "resolution_note": "Cleared browser cache" },
  "memo": "Cache fix confirmed by customer",
  "duration_in_previous_state": "5h30m",
  "trail_length": 6
}
```

Subscribe via:

```smql
SUBSCRIBE TO SupportTicket.transitions
  WHERE TO IN { resolved, escalated }
  DELIVER TO WEBHOOK "https://api.example.com/events"
```

---

## 13. Error Handling

SMQL errors are structured, specific, and actionable. No generic "query failed" messages.

### 13.1 — Transition Errors

```
TransitionDenied {
  instance   : SupportTicket/tk_00123
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
      actual  : ACTOR = Agent/a_bob, assignee = Agent/a_emily
      hint    : "Only the assigned agent or an admin can resolve this ticket"
    }
  ]
}
```

### 13.2 — Spawn Errors

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

### 13.3 — In SDKs

```typescript
import { TransitionDeniedError } from '@smql/client';

try {
  await ticket.transition('resolved', {
    with: { resolution_note: 'Fixed' },
  });
} catch (err) {
  if (err instanceof TransitionDeniedError) {
    console.log(`Cannot resolve: ${err.failures.length} guard(s) failed`);
    err.failures.forEach(f => {
      console.log(`  - ${f.guard}: ${f.hint}`);
    });
    // Output:
    //   Cannot resolve: 1 guard(s) failed
    //     - ACTOR == assignee: Only the assigned agent or an admin can resolve this ticket
  }
}
```

---

## 14. Quick Reference Card

### Spawn

```smql
SPAWN Machine { field: value, ... }
SPAWN Machine { ... } THEN TRANSITION TO state
SPAWN BATCH Machine FROM [ {...}, {...} ]
```

### Transition

```smql
TRANSITION Instance TO state
TRANSITION Instance TO state WITH { field: value }
TRANSITION Instance TO state WITH { ... } MEMO "note" AS actor
TRY TRANSITION Instance TO state
TRANSITION Instance THROUGH [state1, state2, state3]
TRANSITION ALL Machine WHERE condition TO state AS actor
```

### Query

```smql
GET Instance
FIND Machine WHERE condition SORT BY field LIMIT n
FIND Machine WHERE STATE IS state
FIND Machine WHERE STUCK_IN(state, > duration)
FIND Machine WHERE TIMEOUT_REMAINING < duration
FIND Machine WHERE HAS_VISITED(state)
FIND Machine WHERE NEVER_VISITED(state)
FIND Machine WHERE ALIVE / TERMINATED
```

### Trail

```smql
TRAIL OF Instance
FIND Machine WHERE TRAIL CONTAINS (state_a -> state_b)
FIND Machine WHERE TRAIL.count(state) >= n
```

### Temporal

```smql
elapsed_in_state()
entered_state_at()
duration_in(state)
total_lifecycle_duration()
transition_time(state_a, state_b)
elapsed_since(state)
```

### Aggregation

```smql
AGGREGATE Machine MEASURE function GROUP BY field
FUNNEL Machine THROUGH [states] WHERE condition
PATHS OF Machine WHERE condition LIMIT n
COMPARE PATHS OF Machine SEGMENT BY field
```

### Schema

```smql
DEFINE MACHINE Name ( ... )
ALTER MACHINE Name ADD STATE ...
ALTER MACHINE Name REMOVE STATE ... MIGRATE INSTANCES TO ...
ALTER MACHINE Name MODIFY TRANSITION ...
ALTER MACHINE Name ADD DATA { ... } BACKFILL ...
```

---

## Appendix A — Reserved Words

```
DEFINE  MACHINE  STATES  INITIAL  TERMINAL  DATA  TRANSITIONS
GUARD  ACTION  MUTATE  TIMEOUT  PROMPT  MEMO  RESTRICT
SPAWN  TRANSITION  TRY  THROUGH  CASCADE  BATCH
FIND  GET  WHERE  SELECT  SORT  LIMIT  OFFSET
STATE  STUCK_IN  TIMEOUT_REMAINING  HAS_VISITED  NEVER_VISITED
ALIVE  TERMINATED  TRAIL  PATHS  FUNNEL  AGGREGATE
COMPARE  SEGMENT  MEASURE  GROUP  AVG  COUNT  SUM  MIN  MAX
PERCENTILE  SUBSCRIBE  DELIVER  EMIT  NOTIFY  LOG  WEBHOOK
SIGNAL  SPAWN  SELF  ACTOR  ANY  EXCEPT  FROM  TO  IN  AS
OR  AND  NOT  IS  SET  WITH  BETWEEN  BEFORE  AFTER
ALTER  ADD  REMOVE  MODIFY  MIGRATE  BACKFILL  RENAME
DEFINE ROLES  CAN  EXTENDS  VIEW  PROJECTION  MATERIALIZED
REFRESH  HOOKS  BEFORE  AFTER  EACH  ON  ENTER  EXIT  DWELL
PARENT  CHILDREN  REF  LIST  SET  MAP  ENUM  REQUIRED  OPTIONAL
DEFAULT  RANGE  PATTERN  UNIQUE  NOW  TODAY  UPGRADE  INCREMENT
```

---

## Appendix B — Grammar (Simplified EBNF)

```ebnf
machine_def     ::= 'DEFINE' 'MACHINE' IDENT '(' machine_body ')'
machine_body    ::= (data_block | states_block | initial_block |
                     terminal_block | transitions_block | children_block |
                     hooks_block | roles_block)*
states_block    ::= 'STATES' '{' IDENT (',' IDENT)* '}'
initial_block   ::= 'INITIAL' 'STATE' IDENT
terminal_block  ::= 'TERMINAL' 'STATES' '{' IDENT (',' IDENT)* '}'
transition_def  ::= source '->' target '{' transition_clause* '}'
source          ::= IDENT | 'ANY'
target          ::= IDENT
transition_clause ::= guard | action | mutate | timeout | prompt | memo | restrict
guard           ::= 'GUARD' ':' expression
action          ::= 'ACTION' ':' action_expr
timeout         ::= 'TIMEOUT' ':' duration '->' IDENT
spawn_stmt      ::= 'SPAWN' IDENT '{' field_assignments '}'
transition_stmt ::= 'TRANSITION' instance_ref 'TO' IDENT transition_opts*
query_stmt      ::= 'FIND' IDENT where_clause? sort_clause? limit_clause?
trail_stmt      ::= 'TRAIL' 'OF' instance_ref
aggregate_stmt  ::= 'AGGREGATE' IDENT measure_clause group_clause?
```

---

*SMQL Language Specification — Draft v0.1.0*
*Designed for developers who believe data has a lifecycle.*