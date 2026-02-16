# What is SMQL?

**SMQL (State Machine Query Language)** is a database engine purpose-built for data that has a lifecycle. Instead of modeling state transitions in application code on top of a general-purpose database, SMQL makes state machines a first-class primitive.

## The Problem

Every non-trivial application manages entities that move through states: orders go from `draft` to `paid` to `shipped`; support tickets go from `open` to `resolved` to `closed`; CI pipelines go from `queued` to `running` to `passed`.

In traditional databases, you model this with a `status` column and implement transition logic, guards, and audit trails in application code. This leads to:

- **Scattered validation** — guard logic lives in controllers, services, and middleware
- **Missing audit trails** — "who moved this to resolved and when?" requires custom logging
- **Inconsistent transitions** — different code paths allow different state changes
- **No timeout handling** — "auto-close after 7 days" requires external cron jobs
- **Brittle queries** — "how many tickets are stuck in review for > 24h?" requires timestamp math

## Three Principles

### 1. Declare Intent, Not Mechanics

A machine definition reads like a specification. States, transitions, guards, and actions are declared together, not scattered across files.

```sql
DEFINE MACHINE SupportTicket (
  DATA {
    subject  : TEXT -> REQUIRED
    assignee : REF(Agent) -> OPTIONAL
  }

  STATES { open, triaged, in_progress, resolved, closed }
  INITIAL STATE open
  TERMINAL STATES { closed }

  TRANSITIONS {
    open -> triaged {
      GUARD  : assignee IS SET
      ACTION : NOTIFY(assignee, "ticket.assigned")
    }
  }
)
```

### 2. Transitions Are First-Class

A transition is not just a status update. It carries guards (preconditions), mutations (data changes), actions (side effects), and an immutable trail entry. The engine enforces all of these atomically.

### 3. Time Is Native

Timeouts are part of the transition definition. The engine manages timers internally — no cron jobs, no polling, no external schedulers.

```sql
in_progress -> resolved {
  TIMEOUT: 7d -> closed
}
```

## How It Compares

| Feature | SQL + App Code | Workflow Engines | Event Sourcing | SMQL |
|---------|---------------|-----------------|---------------|------|
| State definition | Column + enum | BPMN XML | Event types | `STATES { ... }` |
| Transition rules | Application code | Conditions | Sagas | `GUARD : ...` |
| Audit trail | Custom logging | Built-in | Event log | `TRAIL OF` |
| Timeouts | Cron jobs | Timer events | Scheduled | `TIMEOUT: 7d` |
| Query by state | `WHERE status = ?` | API call | Projection | `STATE IS open` |
| Schema evolution | ALTER TABLE + migration | Redeploy | New event version | `ALTER MACHINE` |
| Learning curve | Low | High | High | Low |

## Next Steps

- [Why SMQL?](./why-smql) — concrete use cases
- [Quick Start](./quick-start) — install, define, spawn, transition in 5 minutes
- [Key Concepts](./key-concepts) — machines, instances, states, transitions, trails
