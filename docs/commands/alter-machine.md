# ALTER MACHINE

`ALTER MACHINE` evolves a machine's schema without losing existing instances. Operations are applied sequentially -- each operation depends on the result of prior operations.

## Syntax

```sql
ALTER MACHINE MachineName (
  ADD STATE new_state
  REMOVE STATE old_state
  ADD TRANSITION source -> target { ... }
  REMOVE TRANSITION source -> target
  ADD DATA { field : TYPE -> constraints }
  REMOVE DATA field_name
)
```

## Operations

### ADD STATE

Add a new state to the machine:

```sql
ALTER MACHINE SupportTicket (
  ADD STATE escalated
)
```

### REMOVE STATE

Remove a state. All transitions involving this state are also removed. Instances currently in this state must be migrated first.

```sql
ALTER MACHINE SupportTicket (
  REMOVE STATE reopened
)
```

::: danger
REMOVE STATE also removes the state from ANY/EXCEPT lists and terminal_states. Ensure no instances are in this state before removing.
:::

### ADD TRANSITION

Add a new transition:

```sql
ALTER MACHINE SupportTicket (
  ADD TRANSITION triaged -> escalated {
    GUARD : priority == "critical"
  }
)
```

### REMOVE TRANSITION

Remove an existing transition:

```sql
ALTER MACHINE SupportTicket (
  REMOVE TRANSITION reopened -> in_progress
)
```

### ADD DATA

Add a new data field with BACKFILL for existing instances:

```sql
ALTER MACHINE SupportTicket (
  ADD DATA { severity : INT -> DEFAULT(0) }
  BACKFILL { severity: 0 }
)
```

### REMOVE DATA

Remove a data field:

```sql
ALTER MACHINE SupportTicket (
  REMOVE DATA satisfaction
)
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
ALTER MACHINE SupportTicket (
  ADD STATE escalated
  ADD TRANSITION triaged -> escalated {
    GUARD : priority == "critical"
  }
)
```

::: warning
Schema migrations skip version checks for affected instances to avoid conflicts during bulk updates.
:::
