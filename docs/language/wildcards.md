# Wildcards

Wildcard transitions use `ANY` to define transitions from multiple source states at once.

## ANY

`ANY` matches all non-terminal states as the source:

```sql
ANY -> cancelled {
  ACTION : EMIT("order.cancelled", { order: SELF })
}
```

This means every non-terminal state can transition to `cancelled`.

## EXCEPT FROM

Use `EXCEPT FROM` to exclude specific states from the wildcard:

```sql
ANY -> cancelled {
  EXCEPT FROM { shipped, delivered, returned }
  ACTION : EMIT("order.cancelled", { order: SELF })
}
```

This allows transition to `cancelled` from any state **except** `shipped`, `delivered`, and `returned`.

## GROUP

Groups name a set of states for use as transition sources:

```sql
ANY -> triaged {
  EXCEPT FROM { open, closed }
  GUARD  : ACTOR.role IN ("admin", "supervisor")
  MUTATE : priority = critical
  ACTION : LOG("Escalated by {ACTOR}")
}
```

::: tip
`ANY` automatically excludes terminal states. You don't need to list them in `EXCEPT FROM`.
:::

::: warning
Wildcard transitions have lower priority than explicit transitions. If both `open -> triaged` and `ANY -> triaged` are defined, the explicit one takes precedence when transitioning from `open`.
:::
