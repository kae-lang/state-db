# DEFINE MACHINE

The `DEFINE MACHINE` statement creates a new machine schema in the catalog.

## Syntax

```sql
DEFINE MACHINE MachineName (
  DATA { ... }
  STATES { state1, state2, ... }
  INITIAL STATE state_name
  TERMINAL STATES { state1, state2 }
  CHILDREN { ... }          -- optional
  TRANSITIONS { ... }
  HOOKS { ... }             -- optional
)
```

## Components

### DATA Block
Declares typed data fields that every instance carries. See [Data Types](./data-types) and [Constraints](./constraints).

```sql
DATA {
  customer_id    : UUID        -> REQUIRED
  subject        : TEXT        -> REQUIRED, MAX(200)
  priority       : ENUM(low, medium, high, critical) -> DEFAULT(medium)
  assignee       : REF(Agent)  -> OPTIONAL
  tags           : SET(TEXT)   -> DEFAULT({})
  satisfaction   : INT         -> RANGE(1, 5), OPTIONAL
}
```

### STATES Block
Lists all possible states. State names are identifiers (lowercase, underscores allowed).

```sql
STATES { open, triaged, in_progress, waiting_on_customer, resolved, closed, reopened }
```

### INITIAL STATE
The state where new instances begin. Must be listed in STATES.

### TERMINAL STATES
States from which no further transitions are possible. Terminal instances are effectively archived.

### CHILDREN Block
Declares parent-child relationships. See [Composition](../composition/overview).

```sql
CHILDREN {
  items    : LIST(LineItem)    -> MIN(1)
  shipment : OPTIONAL(Shipment)
}
```

### PARENT Declaration
For child machines, declares the parent type:

```sql
DEFINE MACHINE LineItem (
  PARENT : Order
  ...
)
```

### TRANSITIONS Block
Defines state transitions with optional guards, mutations, actions, and timeouts. See [States & Transitions](./states-and-transitions).

### HOOKS Block
Declares lifecycle hooks. See [Hooks](./hooks).

## Complete Example

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

  STATES { open, triaged, in_progress, waiting_on_customer, resolved, closed, reopened }
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

    in_progress -> resolved {
      GUARD  : resolution_note IS SET
      GUARD  : ACTOR == assignee OR ACTOR.role == "admin"
      TIMEOUT: 7d -> closed
      ACTION : NOTIFY(customer_id, "ticket.resolved")
    }

    resolved -> closed {
      GUARD : elapsed_since(resolved) >= 7d OR ACTOR.role == "admin"
    }
  }
)
```

::: tip
Machine names are PascalCase by convention (e.g., `SupportTicket`, `LineItem`). State names are snake_case.
:::

::: warning
Redefining an existing machine replaces it. Use [`ALTER MACHINE`](../commands/alter-machine) for production schema evolution.
:::
