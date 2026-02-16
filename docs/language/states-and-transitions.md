# States & Transitions

## States

States represent positions in an entity's lifecycle. They are declared in the `STATES` block:

```sql
STATES { open, triaged, in_progress, waiting_on_customer, resolved, closed, reopened }
```

### Initial State

Every machine must declare exactly one initial state. New instances begin in this state.

```sql
INITIAL STATE open
```

### Terminal States

Terminal states are endpoints -- once an instance reaches a terminal state, no further transitions are allowed.

```sql
TERMINAL STATES { closed }
```

You can have multiple terminal states:

```sql
TERMINAL STATES { delivered, cancelled, returned }
```

## Transitions

Transitions are declared in the `TRANSITIONS` block. Each transition specifies a source state, a target state, and optional clauses.

### Basic Syntax

```sql
TRANSITIONS {
  source -> target {
    GUARD   : <expression>
    MUTATE  : <field> = <expression>
    ACTION  : <action>
    TIMEOUT : <duration> -> <state>
  }
}
```

All clauses inside the braces are optional. An empty transition is valid:

```sql
pending -> running {}
```

### Multiple Guards

Multiple `GUARD` lines are combined with AND -- all must pass:

```sql
in_progress -> resolved {
  GUARD : resolution_note IS SET
  GUARD : ACTOR == assignee OR ACTOR.role == "admin"
}
```

### Transition Anatomy

When a transition executes, the engine follows this pipeline:

1. **Validate** -- target state exists and transition is defined
2. **Guard evaluation** -- all guards must pass
3. **BEFORE hooks** -- sync hooks that can reject
4. **Mutation** -- data fields are updated
5. **State change** -- instance moves to the new state
6. **Trail entry** -- immutable record is appended
7. **Timer management** -- old timers cancelled, new ones set
8. **AFTER hooks** -- async, fire-and-forget
9. **Actions** -- NOTIFY, LOG, EMIT, WEBHOOK
10. **ON ENTER/EXIT hooks** -- state-specific hooks fire

### Comments

Use `--` for inline comments in transition blocks:

```sql
payment_failed -> placed {
  -- retry payment
}
```

::: tip
Transitions from terminal states are not allowed. The engine rejects any attempt to transition an instance that is in a terminal state.
:::
