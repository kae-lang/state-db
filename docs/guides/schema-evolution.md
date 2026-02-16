# Schema Evolution with ALTER MACHINE

This guide walks through a realistic schema evolution scenario using SMQL's `ALTER MACHINE` command. You will start with a simple task management machine, then evolve its schema over multiple iterations -- adding states, transitions, data fields with backfill, and removing obsolete states -- all while the machine has live instances in production.

## Prerequisites

Start the SMQL server:

```bash
smql-server --port 8080
```

---

## Step 1: Start with a Simple Machine

Define a basic task tracker with three states:

```sql
DEFINE MACHINE Task (
  DATA {
    title    : TEXT -> REQUIRED
    priority : INT  -> DEFAULT(3)
  }

  STATES { open, in_progress, done }
  INITIAL STATE open
  TERMINAL STATES { done }

  TRANSITIONS {
    open -> in_progress {}
    in_progress -> done {}
    in_progress -> open {}
  }
)
```

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "DEFINE MACHINE Task ( DATA { title: TEXT -> REQUIRED, priority: INT -> DEFAULT(3) } STATES { open, in_progress, done } INITIAL STATE open TERMINAL STATES { done } TRANSITIONS { open -> in_progress {} in_progress -> done {} in_progress -> open {} } )"}'
```

```json
{
  "success": true,
  "result": { "action": "machine_defined" }
}
```

The machine starts at version 1. Now create some instances:

```bash
# Spawn 3 tasks
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "SPAWN Task { title: \"Implement login\" }"}'

curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "SPAWN Task { title: \"Fix CSS bug\" }"}'

curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "SPAWN Task { title: \"Write tests\" }"}'
```

Move one task to `in_progress`:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION Task \"<task_1_id>\" TO in_progress"}'
```

You now have 3 tasks: 2 in `open`, 1 in `in_progress`. The machine is at version 1.

---

## Step 2: Add a New State

Requirements change: tasks can now be blocked by external dependencies. Add a `blocked` state.

```sql
ALTER MACHINE Task
  ADD STATE blocked
```

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "ALTER MACHINE Task ADD STATE blocked"}'
```

```json
{
  "success": true,
  "result": {
    "action": "machine_altered",
    "machine": "Task",
    "new_version": 2,
    "operations_applied": 1,
    "instances_migrated": 0
  }
}
```

Key observations:
- **new_version: 2** -- The catalog auto-incremented the version.
- **operations_applied: 1** -- One operation was executed.
- **instances_migrated: 0** -- No existing instances needed migration. Adding a state is a schema-only change.

The state exists now but has no transitions to or from it yet. You cannot reach it until you add transitions.

---

## Step 3: Add Transitions to the New State

Add transitions so tasks can move to and from `blocked`:

```sql
ALTER MACHINE Task
  ADD TRANSITION in_progress -> blocked {}
  ADD TRANSITION blocked -> in_progress {}
```

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "ALTER MACHINE Task ADD TRANSITION in_progress -> blocked {} ADD TRANSITION blocked -> in_progress {}"}'
```

```json
{
  "success": true,
  "result": {
    "action": "machine_altered",
    "machine": "Task",
    "new_version": 3,
    "operations_applied": 2,
    "instances_migrated": 0
  }
}
```

Two operations applied in one ALTER command. Each operation is validated against the machine definition as it exists after the previous operation. This means the second `ADD TRANSITION blocked -> in_progress` succeeds because the `blocked` state was added in Step 2 (version 2) and is already in the definition.

Now you can block and unblock tasks:

```bash
# Block a task that is in_progress
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION Task \"<task_1_id>\" TO blocked"}'

# Unblock it later
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION Task \"<task_1_id>\" TO in_progress"}'
```

---

## Step 4: Add a Data Field with BACKFILL

The team wants to categorize tasks. Add a `category` field and backfill all existing instances with a default value:

```sql
ALTER MACHINE Task
  ADD DATA category : TEXT -> OPTIONAL
    BACKFILL "general"
```

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "ALTER MACHINE Task ADD DATA category : TEXT -> OPTIONAL BACKFILL \"general\""}'
```

```json
{
  "success": true,
  "result": {
    "action": "machine_altered",
    "machine": "Task",
    "new_version": 4,
    "operations_applied": 1,
    "instances_migrated": 3
  },
  "warnings": [
    "Backfilled field 'category' on 3 instance(s)"
  ]
}
```

Key observations:
- **instances_migrated: 3** -- All 3 existing task instances were updated.
- The **warnings** array reports what the migration did.
- Every existing instance now has `category: "general"` in its data.

### Backfill vs Default

There are two ways to populate a new field on existing instances:

| Approach | Syntax | Behavior |
|---|---|---|
| **BACKFILL** | `ADD DATA field : TYPE BACKFILL expression` | Evaluates the expression and writes the value to all existing instances immediately. |
| **DEFAULT** | `ADD DATA field : TYPE -> DEFAULT(value)` | Sets the default for new instances. Also backfills existing instances with the default value on ALTER. |
| **Neither** | `ADD DATA field : TYPE -> OPTIONAL` | Adds the field to the schema only. Existing instances get no value (the field is absent/null). |

For REQUIRED fields without a DEFAULT, you must provide a BACKFILL expression. Otherwise the ALTER is rejected:

```json
{
  "success": false,
  "error": "Adding REQUIRED field 'category' without DEFAULT or BACKFILL expression"
}
```

---

## Step 5: Add a Field with DEFAULT

Add a `status_note` field with a default value:

```sql
ALTER MACHINE Task
  ADD DATA status_note : TEXT -> DEFAULT("none")
```

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "ALTER MACHINE Task ADD DATA status_note : TEXT -> DEFAULT(\"none\")"}'
```

```json
{
  "success": true,
  "result": {
    "action": "machine_altered",
    "machine": "Task",
    "new_version": 5,
    "operations_applied": 1,
    "instances_migrated": 3
  },
  "warnings": [
    "Set default for field 'status_note' on 3 instance(s)"
  ]
}
```

When a field has `DEFAULT(value)` and no explicit BACKFILL, the ALTER operation uses the default value to backfill all existing instances.

---

## Step 6: Remove an Obsolete State

After some time, the team decides the `open -> in_progress -> open` cycle is not useful. Tasks should not go back to `open` once started. Remove the `in_progress -> open` transition first, then consider whether to remove a state entirely.

### Remove a Transition

```sql
ALTER MACHINE Task
  REMOVE TRANSITION in_progress -> open
```

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "ALTER MACHINE Task REMOVE TRANSITION in_progress -> open"}'
```

```json
{
  "success": true,
  "result": {
    "action": "machine_altered",
    "machine": "Task",
    "new_version": 6,
    "operations_applied": 1,
    "instances_migrated": 0
  }
}
```

Tasks can no longer move from `in_progress` back to `open`.

### Remove a State with Instance Migration

Suppose the team also decides to remove the `blocked` state (it was underused). Any tasks currently in `blocked` must be migrated to another state:

```sql
ALTER MACHINE Task
  REMOVE STATE blocked MIGRATE TO in_progress
```

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "ALTER MACHINE Task REMOVE STATE blocked MIGRATE TO in_progress"}'
```

```json
{
  "success": true,
  "result": {
    "action": "machine_altered",
    "machine": "Task",
    "new_version": 7,
    "operations_applied": 1,
    "instances_migrated": 1
  },
  "warnings": [
    "Migrated 1 instance(s) from 'blocked' to 'in_progress'",
    "Removed 2 transition(s) involving state 'blocked'"
  ]
}
```

This operation does several things atomically:
1. **Migrates instances** -- The 1 task in `blocked` is moved to `in_progress`.
2. **Removes the state** from the machine definition.
3. **Removes related transitions** -- Both `in_progress -> blocked` and `blocked -> in_progress` are automatically cleaned up.
4. **Cleans up ANY except lists** -- If any wildcard transitions had `blocked` in their `EXCEPT FROM` list, it is removed.

### Constraints on State Removal

You cannot remove certain states:

```bash
# Cannot remove the initial state
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "ALTER MACHINE Task REMOVE STATE open MIGRATE TO in_progress"}'
```

```json
{
  "success": false,
  "error": "Cannot remove initial state 'open' from machine 'Task'"
}
```

You also cannot migrate a state to itself:

```json
{
  "success": false,
  "error": "Cannot migrate state 'open' to itself"
}
```

---

## Step 7: Remove a Data Field

Remove the `status_note` field that turned out to be unused:

```sql
ALTER MACHINE Task
  REMOVE DATA status_note
```

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "ALTER MACHINE Task REMOVE DATA status_note"}'
```

```json
{
  "success": true,
  "result": {
    "action": "machine_altered",
    "machine": "Task",
    "new_version": 8,
    "operations_applied": 1,
    "instances_migrated": 3
  },
  "warnings": [
    "Removed field 'status_note' from 3 instance(s)"
  ]
}
```

The field is removed from both the schema and all existing instances.

---

## Step 8: Multi-Operation ALTER

You can combine multiple operations in a single ALTER MACHINE command. Operations are applied sequentially, and each operation is validated against the current state of the definition (including changes from previous operations in the same command).

```sql
ALTER MACHINE Task
  ADD STATE review
  ADD TRANSITION in_progress -> review {}
  ADD TRANSITION review -> done {}
  ADD TRANSITION review -> in_progress {}
```

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "ALTER MACHINE Task ADD STATE review ADD TRANSITION in_progress -> review {} ADD TRANSITION review -> done {} ADD TRANSITION review -> in_progress {}"}'
```

```json
{
  "success": true,
  "result": {
    "action": "machine_altered",
    "machine": "Task",
    "new_version": 9,
    "operations_applied": 4,
    "instances_migrated": 0
  }
}
```

The sequential validation is critical here: `ADD TRANSITION in_progress -> review` succeeds because `ADD STATE review` was applied first. If you reversed the order, the transition would fail validation because `review` would not exist yet.

---

## Version History Summary

| Version | Operations | Instances Migrated |
|---|---|---|
| v1 | Initial definition | -- |
| v2 | ADD STATE blocked | 0 |
| v3 | ADD TRANSITION in_progress -> blocked, ADD TRANSITION blocked -> in_progress | 0 |
| v4 | ADD DATA category (BACKFILL "general") | 3 |
| v5 | ADD DATA status_note (DEFAULT "none") | 3 |
| v6 | REMOVE TRANSITION in_progress -> open | 0 |
| v7 | REMOVE STATE blocked MIGRATE TO in_progress | 1 |
| v8 | REMOVE DATA status_note | 3 |
| v9 | ADD STATE review + 3 transitions | 0 |

---

## How ALTER MACHINE Works Internally

1. **Load** the current machine definition from the catalog.
2. **For each operation**, in order:
   a. **Validate** the operation against the current (possibly already mutated) definition.
   b. **Apply** the schema change to the in-memory definition.
   c. **Migrate** any affected instances in storage (state migrations, data backfills, field removals).
3. **Update** the catalog with the new definition (increments the version).
4. **Return** the result with `new_version`, `operations_applied`, `instances_migrated`, and any `warnings`.

### Key Design Decisions

- **Sequential validation**: Each operation sees the effects of all prior operations in the same command. This allows `ADD STATE x` followed by `ADD TRANSITION a -> x` in one command.
- **Instance migration skips version checks**: Storage-level migrations (state migration, bulk field updates) bypass optimistic concurrency version checks. This is necessary because schema migration is an administrative operation that must succeed regardless of concurrent activity.
- **Atomic per-operation**: Each operation either fully succeeds or fails. If operation 3 out of 5 fails validation, operations 1 and 2 have already been applied. The command returns an error, but the catalog retains the partial changes. Design your ALTER commands with this in mind, or use single-operation ALTER commands for safety.
- **Version auto-increment**: Every successful ALTER MACHINE bumps the version by 1, regardless of how many operations were applied.

### ALTER Operation Reference

| Operation | Syntax | Migrates Instances? |
|---|---|---|
| ADD STATE | `ADD STATE name` | No |
| REMOVE STATE | `REMOVE STATE name MIGRATE TO target` | Yes -- moves instances to target state |
| ADD TRANSITION | `ADD TRANSITION from -> to { guards/actions }` | No |
| REMOVE TRANSITION | `REMOVE TRANSITION from -> to` | No |
| MODIFY TRANSITION | `MODIFY TRANSITION from -> to { new guards/actions }` | No |
| ADD DATA | `ADD DATA field : TYPE -> constraints` | Yes -- backfills with DEFAULT or BACKFILL |
| ADD DATA + BACKFILL | `ADD DATA field : TYPE BACKFILL expression` | Yes -- evaluates expression for all instances |
| REMOVE DATA | `REMOVE DATA field_name` | Yes -- removes field from all instances |
| BACKFILL | `BACKFILL field WITH expression` | Yes -- updates field on all instances |
