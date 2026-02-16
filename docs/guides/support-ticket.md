# Full Support Ticket Walkthrough

This guide walks through the complete lifecycle of a support ticket using SMQL, from defining the machine to resolving and closing tickets. You will use `curl` commands against the SMQL HTTP server running at `localhost:8080`.

## Prerequisites

Start the SMQL server:

```bash
smql-server --port 8080
```

---

## Step 1: Define the Machine

The SupportTicket machine captures the full lifecycle of a customer support request. It has 7 states, typed data fields with validation constraints, guarded transitions, timeouts, and a wildcard escalation path.

```sql
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

Register it with the server:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "DEFINE MACHINE SupportTicket ( DATA { customer_id: UUID -> REQUIRED, subject: TEXT -> REQUIRED, MAX(200), description: TEXT -> REQUIRED, priority: ENUM(low, medium, high, critical) -> DEFAULT(medium), assignee: REF(Agent) -> OPTIONAL, tags: SET(TEXT) -> DEFAULT({}), satisfaction: INT -> RANGE(1, 5), OPTIONAL, resolution_note: TEXT -> OPTIONAL } STATES { open, triaged, in_progress, waiting_on_customer, resolved, closed, reopened } INITIAL STATE open TERMINAL STATES { closed } TRANSITIONS { open -> triaged { GUARD: assignee IS SET ACTION: NOTIFY(assignee, \"ticket.assigned\") } triaged -> in_progress { GUARD: ACTOR == assignee OR ACTOR.role == \"admin\" } in_progress -> waiting_on_customer { GUARD: ACTOR == assignee TIMEOUT: 72h -> resolved ACTION: NOTIFY(customer_id, \"ticket.needs_response\") } waiting_on_customer -> in_progress { GUARD: ACTOR.id == customer_id OR ACTOR == assignee } in_progress -> resolved { GUARD: resolution_note IS SET GUARD: ACTOR == assignee OR ACTOR.role == \"admin\" TIMEOUT: 7d -> closed ACTION: NOTIFY(customer_id, \"ticket.resolved\") } resolved -> reopened { GUARD: ACTOR.id == customer_id GUARD: elapsed_since(resolved) < 30d } reopened -> in_progress { GUARD: assignee IS SET } resolved -> closed { GUARD: elapsed_since(resolved) >= 7d OR ACTOR.role == \"admin\" } ANY -> triaged { EXCEPT FROM { open, closed } GUARD: ACTOR.role IN (\"admin\", \"supervisor\") MUTATE: priority = critical ACTION: LOG(\"Escalated by {ACTOR}\") } } )"}'
```

```json
{
  "success": true,
  "result": { "action": "machine_defined" }
}
```

### Understanding the Data Schema

Each data field has a type and constraints:

| Field | Type | Constraints | Purpose |
|---|---|---|---|
| `customer_id` | `UUID` | `REQUIRED` | Links to the customer who opened the ticket |
| `subject` | `TEXT` | `REQUIRED, MAX(200)` | Short summary, capped at 200 characters |
| `priority` | `ENUM(...)` | `DEFAULT(medium)` | Auto-set to `medium` if not provided |
| `assignee` | `REF(Agent)` | `OPTIONAL` | Reference to the agent working the ticket |
| `tags` | `SET(TEXT)` | `DEFAULT({})` | Defaults to empty set |
| `satisfaction` | `INT` | `RANGE(1, 5), OPTIONAL` | Customer satisfaction score, only valid 1-5 |
| `resolution_note` | `TEXT` | `OPTIONAL` | Required by guard before resolving |

---

## Step 2: Spawn a Ticket

When a customer submits a support request, spawn a new instance. Required fields must be provided; optional fields with defaults are filled automatically.

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "SPAWN SupportTicket { customer_id: \"a1b2c3d4-e5f6-7890-abcd-ef1234567890\", subject: \"Login broken after update\", description: \"Cannot log in since the v2.3 update\" }"
  }'
```

```json
{
  "success": true,
  "result": {
    "id": "01JMABCDEF1234567890ABCDEF",
    "machine": "SupportTicket",
    "state": "open",
    "data": {
      "customer_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
      "subject": "Login broken after update",
      "description": "Cannot log in since the v2.3 update",
      "priority": "medium",
      "tags": []
    },
    "created_at": "2026-02-16T10:00:00Z",
    "updated_at": "2026-02-16T10:00:00Z",
    "state_entered_at": "2026-02-16T10:00:00Z",
    "trail_length": 1,
    "version": 1
  }
}
```

Notice that `priority` was automatically set to `"medium"` and `tags` to an empty set. The instance starts in the `open` state as declared by `INITIAL STATE open`.

### Spawn Validation

If you omit a `REQUIRED` field, the spawn is rejected:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "SPAWN SupportTicket {}"}'
```

```json
{
  "success": false,
  "error": "Spawn rejected: missing required field 'customer_id'"
}
```

---

## Step 3: Assign and Triage

To move from `open` to `triaged`, the guard requires `assignee IS SET`. Use the `WITH` clause to provide the assignee at transition time.

The `ACTOR` is the person performing the transition, provided via `AS`. Since ACTOR evaluates to a map `{id: "...", role: "..."}`, the assignee must also be set to a matching map so that later guards like `ACTOR == assignee` can compare correctly.

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION SupportTicket \"01JMABCDEF1234567890ABCDEF\" TO triaged WITH { assignee: {id: \"agent_1\"} } AS \"agent_1\""
  }'
```

```json
{
  "success": true,
  "result": {
    "from_state": "open",
    "to_state": "triaged",
    "instance": {
      "id": "01JMABCDEF1234567890ABCDEF",
      "machine": "SupportTicket",
      "state": "triaged",
      "data": {
        "assignee": { "id": "agent_1" },
        "customer_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
        "description": "Cannot log in since the v2.3 update",
        "priority": "medium",
        "subject": "Login broken after update",
        "tags": []
      },
      "trail_length": 2,
      "version": 2
    }
  }
}
```

The `ACTION: NOTIFY(assignee, "ticket.assigned")` fires asynchronously to notify the assigned agent.

### Guard Failure: Missing Assignee

Without `WITH { assignee: ... }`, the guard rejects the transition:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION SupportTicket \"01JMABCDEF1234567890ABCDEF\" TO triaged"
  }'
```

```json
{
  "success": false,
  "error": "Transition denied: guard failed: assignee IS SET"
}
```

---

## Step 4: Start Working

The `triaged -> in_progress` transition has this guard:

```sql
GUARD : ACTOR == assignee OR ACTOR.role == "admin"
```

Only the assigned agent or an admin can pick up the ticket:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION SupportTicket \"01JMABCDEF1234567890ABCDEF\" TO in_progress AS \"agent_1\""
  }'
```

```json
{
  "success": true,
  "result": {
    "from_state": "triaged",
    "to_state": "in_progress",
    "instance": {
      "state": "in_progress",
      "trail_length": 3,
      "version": 3
    }
  }
}
```

### Guard Failure: Wrong Actor

If someone other than the assignee (and not an admin) tries to pick it up:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION SupportTicket \"01JMABCDEF1234567890ABCDEF\" TO in_progress AS \"random_user\""
  }'
```

```json
{
  "success": false,
  "error": "Transition denied: guard failed: ACTOR == assignee OR ACTOR.role == \"admin\""
}
```

---

## Step 5: Wait for Customer

When the agent needs more information from the customer:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION SupportTicket \"01JMABCDEF1234567890ABCDEF\" TO waiting_on_customer AS \"agent_1\""
  }'
```

```json
{
  "success": true,
  "result": {
    "from_state": "in_progress",
    "to_state": "waiting_on_customer"
  }
}
```

### Understanding the Timeout

This transition has `TIMEOUT: 72h -> resolved`. Once the ticket enters `waiting_on_customer`, SMQL registers a timer. If the customer does not respond within 72 hours, the system automatically transitions the ticket to `resolved` using the System actor. The timeout bypasses guards -- it is a guard-free transition performed by the system.

When the instance leaves `waiting_on_customer` (either by customer response or timeout), the timer is canceled automatically.

### Customer Responds

The customer or the assigned agent can move it back to `in_progress`:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION SupportTicket \"01JMABCDEF1234567890ABCDEF\" TO in_progress AS \"a1b2c3d4-e5f6-7890-abcd-ef1234567890\""
  }'
```

---

## Step 6: Resolve the Ticket

The `in_progress -> resolved` transition requires two guards to pass simultaneously:

```sql
GUARD : resolution_note IS SET
GUARD : ACTOR == assignee OR ACTOR.role == "admin"
```

Both guards are evaluated, and all failures are collected (not just the first one). You must provide the resolution note via `WITH`:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION SupportTicket \"01JMABCDEF1234567890ABCDEF\" TO resolved WITH { resolution_note: \"Cleared browser cache and reset session tokens. Issue resolved.\" } AS \"agent_1\""
  }'
```

```json
{
  "success": true,
  "result": {
    "from_state": "in_progress",
    "to_state": "resolved",
    "instance": {
      "state": "resolved",
      "data": {
        "resolution_note": "Cleared browser cache and reset session tokens. Issue resolved."
      },
      "trail_length": 6,
      "version": 6
    }
  }
}
```

Once resolved, the `TIMEOUT: 7d -> closed` timer starts. If nobody reopens the ticket within 7 days, it automatically closes.

---

## Step 7: Close the Ticket

An admin can close the ticket immediately. Otherwise, the 7-day timeout closes it automatically.

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION SupportTicket \"01JMABCDEF1234567890ABCDEF\" TO closed AS \"admin_user\" WITH { satisfaction: 4 }"
  }'
```

The `closed` state is a **terminal state**. Once a ticket is closed, no further transitions are possible.

---

## Wildcard Escalation

The `ANY -> triaged` wildcard transition allows admins and supervisors to escalate a ticket from any non-excluded state:

```sql
ANY -> triaged {
  EXCEPT FROM { open, closed }
  GUARD  : ACTOR.role IN ("admin", "supervisor")
  MUTATE : priority = critical
  ACTION : LOG("Escalated by {ACTOR}")
}
```

Key details:

- **ANY** matches every state in the machine.
- **EXCEPT FROM { open, closed }** excludes tickets that are already open (use the normal triage path) or closed (terminal state).
- **MUTATE** automatically sets `priority = critical` as a side effect of the transition.
- Works from `in_progress`, `waiting_on_customer`, `resolved`, `reopened` -- any state not in the except list.

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION SupportTicket \"01JMABCDEF1234567890ABCDEF\" TO triaged AS \"supervisor_1\""
  }'
```

After this transition, the ticket's priority is `critical` regardless of what it was before.

---

## Querying Tickets

### Find All Open Tickets

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "FIND SupportTicket WHERE STATE IS open"
  }'
```

```json
{
  "success": true,
  "result": {
    "count": 2,
    "instances": [
      { "id": "01JM...", "state": "open", "data": { "priority": "medium" } },
      { "id": "01JM...", "state": "open", "data": { "priority": "high" } }
    ]
  }
}
```

### View the Audit Trail

Every spawn and transition is recorded in an immutable trail:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRAIL OF SupportTicket \"01JMABCDEF1234567890ABCDEF\""
  }'
```

```json
{
  "success": true,
  "result": {
    "count": 6,
    "entries": [
      { "sequence": 0, "from_state": "", "to_state": "open", "actor": null, "timestamp": "2026-02-16T10:00:00Z" },
      { "sequence": 1, "from_state": "open", "to_state": "triaged", "actor": "agent_1", "timestamp": "2026-02-16T10:05:00Z" },
      { "sequence": 2, "from_state": "triaged", "to_state": "in_progress", "actor": "agent_1", "timestamp": "2026-02-16T10:06:00Z" },
      { "sequence": 3, "from_state": "in_progress", "to_state": "waiting_on_customer", "actor": "agent_1", "timestamp": "2026-02-16T10:30:00Z" },
      { "sequence": 4, "from_state": "waiting_on_customer", "to_state": "in_progress", "actor": "a1b2c3d4-...", "timestamp": "2026-02-16T11:00:00Z" },
      { "sequence": 5, "from_state": "in_progress", "to_state": "resolved", "actor": "agent_1", "timestamp": "2026-02-16T11:15:00Z" }
    ]
  }
}
```

Sequence 0 is always the spawn event, with an empty `from_state`.

### Aggregate: Count by State

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "AGGREGATE SupportTicket MEASURE COUNT() GROUP BY state"
  }'
```

```json
{
  "success": true,
  "result": {
    "rows": [
      { "group": { "state": "open" }, "measures": { "count": 5 } },
      { "group": { "state": "in_progress" }, "measures": { "count": 3 } },
      { "group": { "state": "resolved" }, "measures": { "count": 2 } }
    ]
  }
}
```

### Funnel Analysis

Track conversion through the ticket lifecycle:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "FUNNEL SupportTicket THROUGH open, triaged, in_progress, resolved"
  }'
```

```json
{
  "success": true,
  "result": {
    "stages": [
      { "state": "open", "count": 10, "conversion_rate": 1.0 },
      { "state": "triaged", "count": 8, "conversion_rate": 0.8 },
      { "state": "in_progress", "count": 7, "conversion_rate": 0.875 },
      { "state": "resolved", "count": 5, "conversion_rate": 0.714 }
    ]
  }
}
```

---

## Key Concepts Recap

| Concept | How it works in SupportTicket |
|---|---|
| **Guards** | Conditions that must be true before a transition can proceed. Multiple guards on the same transition must ALL pass. |
| **ACTOR** | The person or system performing the transition. Evaluates to a map `{id: "...", role: "..."}`. |
| **WITH clause** | Provides data mutations at transition time (e.g., setting `assignee` or `resolution_note`). |
| **TIMEOUT** | Automatic system-initiated transition after a duration. Bypasses guards. Canceled when the instance leaves the state. |
| **MUTATE** | Automatic data changes applied during a transition (e.g., `priority = critical` on escalation). |
| **ANY wildcard** | Matches all states except those in the `EXCEPT FROM` list. Useful for cross-cutting concerns like escalation and cancellation. |
| **Terminal state** | `closed` is the only terminal state. Once reached, no transitions are possible. |
| **Trail** | Immutable audit log of every spawn and transition, with timestamps, actors, and memos. |
