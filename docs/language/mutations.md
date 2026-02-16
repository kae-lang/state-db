# Mutations

The `MUTATE` clause modifies instance data during a transition.

## Syntax

```sql
source -> target {
  MUTATE : field_name = expression
}
```

## Examples

### Set a Field

```sql
ANY -> triaged {
  MUTATE : priority = critical
}
```

### Using WITH Data

Transitions can receive external data via the `WITH` clause at execution time. The `MUTATE` block can reference these values:

```sql
TRANSITION SupportTicket "01J5..." TO resolved
  WITH { resolution_note: "Fixed the issue" }
```

### SPAWN in MUTATE

The `SPAWN` keyword inside a `MUTATE` clause creates a child instance:

```sql
paid -> fulfilled {
  MUTATE : shipment = SPAWN Shipment { order: SELF }
}
```

This spawns a new `Shipment` instance and stores its reference in the `shipment` field.

::: warning
`SPAWN` in MUTATE requires `{}` even with no initial data: `SPAWN Machine {}`.
:::

## WITH Clause (Execution Time)

The `WITH` clause on a transition command provides data that is merged into the instance:

```sql
TRANSITION SupportTicket "01J5..." TO resolved
  WITH { resolution_note: "Customer confirmed fix" }
```

::: tip
`WITH` uses braces and colons: `WITH { key: value }`, not `WITH key = value`.
:::
