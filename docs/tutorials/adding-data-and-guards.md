# Tutorial 2: Adding Data & Guards

In the [previous tutorial](./your-first-machine), you built a simple machine with no data. Real-world machines need to store information and enforce rules about when transitions can happen. In this tutorial, you'll add both.

## What You'll Build

A `TodoItem` machine that tracks tasks with a title, assignee, priority, and notes. Guards enforce business rules like "only the assignee can complete a task" and "you must add a note before marking it done."

```
          ┌──────────┐
          │  open     │──→ in_progress ──→ done
          └──────────┘         │
               ↑               ↓
               └──── blocked ──┘
```

## Step 1: Define the Machine with Data

The `DATA` block declares typed fields with validation constraints:

```sql
DEFINE MACHINE TodoItem (

  DATA {
    title       : TEXT        -> REQUIRED, MAX(100)
    assignee    : TEXT        -> OPTIONAL
    priority    : ENUM(low, medium, high) -> DEFAULT(medium)
    notes       : TEXT        -> OPTIONAL
    effort_days : INT         -> RANGE(1, 30), OPTIONAL
  }

  STATES { open, in_progress, blocked, done }
  INITIAL STATE open
  TERMINAL STATES { done }

  TRANSITIONS {
    open -> in_progress {
      GUARD : assignee IS SET
    }

    in_progress -> blocked {
      GUARD : notes IS SET
    }

    blocked -> in_progress {}

    in_progress -> done {
      GUARD : notes IS SET
      GUARD : ACTOR.id == assignee
    }

    open -> done {
      GUARD : ACTOR.role == "admin"
    }
  }
)
```

### Data Field Breakdown

| Field | Type | Constraints | Purpose |
|-------|------|-------------|---------|
| `title` | `TEXT` | `REQUIRED, MAX(100)` | Must be provided at spawn, max 100 chars |
| `assignee` | `TEXT` | `OPTIONAL` | Can be set at spawn or later via WITH |
| `priority` | `ENUM(low, medium, high)` | `DEFAULT(medium)` | Auto-set to "medium" if omitted |
| `notes` | `TEXT` | `OPTIONAL` | Free-form notes, used in guard conditions |
| `effort_days` | `INT` | `RANGE(1, 30), OPTIONAL` | Estimated effort, validated to 1-30 |

### Guard Conditions Explained

Each `GUARD` is a boolean expression evaluated before the transition proceeds:

- **`assignee IS SET`** — the field must have a non-null value
- **`notes IS SET`** — prevents transitions without documentation
- **`ACTOR.id == assignee`** — only the assigned person can complete the task
- **`ACTOR.role == "admin"`** — role-based access control

When multiple guards appear on the same transition, **all must pass**.

## Step 2: Spawn with Data

Provide data fields as key-value pairs inside `{}`. Required fields must be present; optional fields with defaults are filled automatically.

::: code-group
```bash [REPL]
> SPAWN TodoItem { title: "Write unit tests", assignee: "alice", effort_days: 5 }
```

```bash [curl]
curl -X POST http://localhost:4200/execute \
  -H 'Content-Type: application/json' \
  -d '{
    "smql": "SPAWN TodoItem { title: \"Write unit tests\", assignee: \"alice\", effort_days: 5 }"
  }'
```
:::

```json
{
  "success": true,
  "result": {
    "id": "01JMTODO00000000000000001A",
    "machine": "TodoItem",
    "state": "open",
    "data": {
      "title": "Write unit tests",
      "assignee": "alice",
      "priority": "medium",
      "effort_days": 5
    },
    "trail_length": 1,
    "version": 1
  }
}
```

Notice that `priority` was automatically set to `"medium"` because we didn't provide it.

### Validation Errors

SMQL validates data at spawn time. If you violate constraints, the spawn is rejected:

**Missing required field:**
```bash
> SPAWN TodoItem {}
```
```json
{ "success": false, "error": "Spawn rejected: missing required field 'title'" }
```

**Value out of range:**
```bash
> SPAWN TodoItem { title: "Test", effort_days: 50 }
```
```json
{ "success": false, "error": "Spawn rejected: field 'effort_days' out of range (1..30)" }
```

**Invalid enum value:**
```bash
> SPAWN TodoItem { title: "Test", priority: "urgent" }
```
```json
{ "success": false, "error": "Spawn rejected: invalid enum value 'urgent' for field 'priority'" }
```

## Step 3: Guards in Action

### Successful Transition

The `open -> in_progress` guard requires `assignee IS SET`. Since we set `assignee: "alice"` at spawn, this passes:

```bash
> TRANSITION "01JMTODO00000000000000001A" TO in_progress
```

```json
{
  "success": true,
  "result": {
    "from_state": "open",
    "to_state": "in_progress"
  }
}
```

### Guard Failure

Try to move to `done` without setting `notes`:

```bash
> TRANSITION "01JMTODO00000000000000001A" TO done AS "alice"
```

```json
{
  "success": false,
  "error": "Transition denied: guard failed: notes IS SET"
}
```

The guard `notes IS SET` failed because we never set the `notes` field.

## Step 4: The WITH Clause

Use `WITH { ... }` to update data fields during a transition. This lets you satisfy guard requirements and update state in a single atomic operation.

```bash
> TRANSITION "01JMTODO00000000000000001A" TO done WITH { notes: "All tests passing" } AS "alice"
```

```json
{
  "success": true,
  "result": {
    "from_state": "in_progress",
    "to_state": "done",
    "instance": {
      "state": "done",
      "data": {
        "title": "Write unit tests",
        "assignee": "alice",
        "priority": "medium",
        "effort_days": 5,
        "notes": "All tests passing"
      }
    }
  }
}
```

Both guards passed:
1. `notes IS SET` — satisfied by the `WITH` clause
2. `ACTOR.id == assignee` — the actor is "alice" and so is the assignee

::: tip
The `WITH` clause data is applied **before** guards are evaluated. This means you can provide a required field and pass its guard in the same command.
:::

## Step 5: The ACTOR System

The `AS` clause sets the **actor** — the person or system performing the transition. ACTOR evaluates to a map with `id` and `role` fields.

```bash
-- AS "alice" sets ACTOR to { id: "alice" }
> TRANSITION "..." TO in_progress AS "alice"

-- AS "admin_user" with role in the actor value
> TRANSITION "..." TO done AS "admin_user"
```

Guards can check actor identity and roles:

```sql
-- Only the assignee can complete
GUARD : ACTOR.id == assignee

-- Only admins can bypass
GUARD : ACTOR.role == "admin"

-- Either the assignee or admin
GUARD : ACTOR == assignee OR ACTOR.role == "admin"
```

### Wrong Actor

If someone other than the assignee tries to complete a task:

```bash
> TRANSITION "01JMTODO00000000000000001A" TO done WITH { notes: "Done" } AS "bob"
```

```json
{
  "success": false,
  "error": "Transition denied: guard failed: ACTOR.id == assignee"
}
```

## Step 6: Blocked → Unblocked Flow

Spawn a new todo and walk through the blocking flow:

```bash
> SPAWN TodoItem { title: "Fix login bug", assignee: "bob" }
```

Save the ID, then:

```bash
-- Start work
> TRANSITION "<id>" TO in_progress

-- Block with a reason (notes required by guard)
> TRANSITION "<id>" TO blocked WITH { notes: "Waiting on API team" }

-- Unblock (no guard on blocked -> in_progress)
> TRANSITION "<id>" TO blocked

-- Complete
> TRANSITION "<id>" TO done WITH { notes: "API fixed, login works" } AS "bob"
```

## Step 7: Admin Override

The `open -> done` transition has `GUARD: ACTOR.role == "admin"`, allowing admins to close tasks directly:

```bash
> SPAWN TodoItem { title: "Cancelled task" }
> TRANSITION "<id>" TO done AS "admin_user"
```

::: warning
This only works if the actor's role is "admin". A regular user trying this path will get a guard failure.
:::

## Step 8: MUTATE — Automatic Data Changes

In the previous tutorial, data only changed via `WITH`. SMQL also supports `MUTATE` — automatic data modifications that happen as part of a transition, without the caller providing them.

Let's extend the machine conceptually (this is what it looks like in a full definition):

```sql
open -> in_progress {
  GUARD  : assignee IS SET
  MUTATE : priority = high
}
```

With this definition, transitioning to `in_progress` would automatically set `priority` to `high` regardless of what the caller provides. The `MUTATE` happens after guards pass, as a side effect of the transition.

::: info
`WITH` is caller-provided data. `MUTATE` is machine-defined automatic data change. Both happen atomically during the transition.
:::

## What You Learned

| Concept | Summary |
|---------|---------|
| `DATA` block | Declares typed fields with validation constraints |
| `REQUIRED` / `OPTIONAL` | Controls whether a field must be provided at spawn |
| `DEFAULT(value)` | Auto-fills a field if not provided |
| `MAX`, `RANGE`, `ENUM` | Type-level validation constraints |
| `GUARD` | Boolean condition that must be true for a transition to proceed |
| `IS SET` / `IS NOT SET` | Null-checking predicates for guard conditions |
| `ACTOR` | The person or system performing the transition |
| `WITH { ... }` | Provides data mutations at transition time |
| `MUTATE` | Automatic data changes defined in the machine |

## Next Step

Your machine now has data and rules, but it doesn't react to time or emit events. In the [next tutorial](./timeouts-and-hooks), you'll add automatic timeouts that fire when things take too long, and hooks that trigger side effects when state changes happen.
