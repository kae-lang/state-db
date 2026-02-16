# Hooks

Hooks allow you to attach lifecycle callbacks to machines. They fire on spawn, state enter/exit, and transitions.

## Syntax

Hooks are declared in a `HOOKS` block inside the machine definition:

```sql
DEFINE MACHINE MyMachine (
  ...
  HOOKS {
    ON SPAWN {
      EMIT("machine.spawned")
    }

    BEFORE EACH TRANSITION {
      LOG("Transition starting")
    }

    AFTER EACH TRANSITION {
      EMIT("machine.transitioned")
    }

    ON ENTER resolved {
      NOTIFY(customer_id, "ticket.resolved")
    }

    ON EXIT in_progress {
      LOG("Left in_progress")
    }
  }
)
```

## Hook Types

### ON SPAWN

Fires when a new instance is created.

```sql
ON SPAWN {
  EMIT("ticket.created")
}
```

### BEFORE EACH TRANSITION

Fires synchronously before every transition. Can reject the transition (treated as a guard failure).

```sql
BEFORE EACH TRANSITION {
  LOG("Validating transition")
}
```

::: warning
`BEFORE EACH TRANSITION` hooks are synchronous and blocking. If they fail, the transition is denied.
:::

### AFTER EACH TRANSITION

Fires asynchronously after every successful transition. Fire-and-forget -- failures do not roll back.

```sql
AFTER EACH TRANSITION {
  EMIT("state.changed")
}
```

### ON ENTER / ON EXIT

Fires when entering or exiting a specific state:

```sql
ON ENTER resolved {
  NOTIFY(customer_id, "ticket.resolved")
}

ON EXIT in_progress {
  LOG("Left in_progress state")
}
```

## Hook Actions

Hooks support the same actions as transitions:

- `LOG("message")` -- structured logging
- `EMIT("event_name")` -- publish to event bus
- `NOTIFY(target, "type")` -- send notification
- `WEBHOOK(url, data)` -- call external endpoint

## Execution Order

1. `BEFORE EACH TRANSITION` (sync, can reject)
2. Guard evaluation
3. Mutation
4. State change
5. `ON EXIT <old_state>` (async)
6. `ON ENTER <new_state>` (async)
7. `AFTER EACH TRANSITION` (async)
8. Transition actions

::: tip
Hooks are declared once in the machine definition and apply to all instances. Use transition-level `ACTION` for transition-specific side effects.
:::
