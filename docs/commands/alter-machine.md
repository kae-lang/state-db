# ALTER MACHINE

`ALTER MACHINE` evolves a machine's schema without losing existing instances. Operations are applied sequentially -- each operation depends on the result of prior operations.

## Syntax

```sql
ALTER MACHINE MachineName
  ADD STATE new_state
  REMOVE STATE old_state MIGRATE TO replacement_state
  ADD TRANSITION source -> target
  REMOVE TRANSITION source -> target
  ADD DATA field : TYPE -> constraints
  REMOVE DATA field_name
```

## Operations

### ADD STATE

Add a new state to the machine:

```sql
ALTER MACHINE SupportTicket
  ADD STATE escalated
```

### REMOVE STATE

Remove a state. All transitions involving this state are also removed. The `MIGRATE TO` clause is mandatory and specifies which state existing instances should be moved to.

```sql
ALTER MACHINE SupportTicket
  REMOVE STATE reopened MIGRATE TO open
```

::: danger
REMOVE STATE also removes the state from ANY/EXCEPT lists and terminal_states. Ensure no instances are in this state before removing.
:::

### ADD TRANSITION

Add a new transition:

```sql
ALTER MACHINE SupportTicket
  ADD TRANSITION triaged -> escalated
```

> **Note:** Guard bodies are not supported in `ALTER ADD TRANSITION`. To add guarded transitions, define them in the original `MACHINE` block or use a new machine version.

### REMOVE TRANSITION

Remove an existing transition:

```sql
ALTER MACHINE SupportTicket
  REMOVE TRANSITION reopened -> in_progress
```

### ADD DATA

Add a new data field with inline BACKFILL for existing instances:

```sql
ALTER MACHINE SupportTicket
  ADD DATA severity : INT -> DEFAULT(0) BACKFILL 0
```

The BACKFILL expression is applied inline after the field definition. For REQUIRED fields without a DEFAULT, you must provide a BACKFILL value.

### REMOVE DATA

Remove a data field:

```sql
ALTER MACHINE SupportTicket
  REMOVE DATA satisfaction
```

## Response

```json
{
  "success": true,
  "result": {
    "action": "machine_altered",
    "machine": "SupportTicket",
    "new_version": 2,
    "operations_applied": 3,
    "instances_migrated": 42
  }
}
```

## Multi-Operation

Operations within a single `ALTER MACHINE` are applied sequentially. Later operations can reference states or transitions added by earlier operations:

```sql
ALTER MACHINE SupportTicket
  ADD STATE escalated
  ADD TRANSITION triaged -> escalated
```

::: warning
Schema migrations skip version checks for affected instances to avoid conflicts during bulk updates.
:::
