# Key Concepts

## Machine

A **machine** is a schema that defines the lifecycle of an entity. It declares data fields, states, transitions, guards, actions, and timeouts. Think of it as a blueprint.

```sql
DEFINE MACHINE SupportTicket (
  DATA { ... }
  STATES { open, triaged, resolved, closed }
  INITIAL STATE open
  TERMINAL STATES { closed }
  TRANSITIONS { ... }
)
```

Machines are registered in the **catalog** and can be evolved with `ALTER MACHINE`.

## Instance

An **instance** is a single entity living within a machine's lifecycle. Each instance has:

- A **ULID** identifier (26-character, time-sortable)
- A current **state**
- **Data** fields matching the machine's schema
- A **version** counter for optimistic concurrency
- Timestamps: `created_at`, `updated_at`, `state_entered_at`

Instances are created with `SPAWN` and moved with `TRANSITION`.

## State

A state represents a position in the lifecycle. Every machine has:

- An **initial state** — where new instances begin
- **Terminal states** — once reached, no further transitions are possible
- **Regular states** — intermediate positions in the lifecycle

```sql
STATES { open, triaged, in_progress, resolved, closed }
INITIAL STATE open
TERMINAL STATES { closed }
```

## Transition

A **transition** moves an instance from one state to another. Transitions can include:

| Component | Purpose | Example |
|-----------|---------|---------|
| **Guard** | Precondition that must be true | `GUARD : assignee IS SET` |
| **Mutate** | Data modification during transition | `MUTATE : priority = critical` |
| **Action** | Side effect after transition | `ACTION : NOTIFY(assignee, "assigned")` |
| **Timeout** | Automatic transition after duration | `TIMEOUT : 7d -> closed` |
| **Memo** | Human-readable note (per execution) | `MEMO "Resolved by customer"` |
| **Actor** | Identity performing the transition | `AS { id: "u1", role: "admin" }` |

```sql
in_progress -> resolved {
  GUARD   : resolution_note IS SET
  GUARD   : ACTOR == assignee OR ACTOR.role == "admin"
  TIMEOUT : 7d -> closed
  ACTION  : NOTIFY(customer_id, "ticket.resolved")
}
```

## Trail

Every instance maintains an immutable **trail** — a complete history of every state change. Each trail entry records:

- **Sequence number** (0-indexed, starting with the spawn event)
- **From state** and **to state**
- **Actor** who performed the transition
- **Memo** text (if provided)
- **Timestamp**

Query the trail with `TRAIL OF`:

```sql
TRAIL OF "01J5X7K2P3Q4R5S6T7U8V9W0XY"
```

## Guard

A **guard** is a boolean expression that must evaluate to `true` for a transition to proceed. Guards can reference:

- Instance data fields: `assignee IS SET`, `priority == "critical"`
- The actor: `ACTOR.role == "admin"`
- Built-in functions: `elapsed_since(resolved) < 30d`
- Child predicates: `ALL(items, STATE IS confirmed)`

Multiple guards on a transition are combined with AND — all must pass.

## Actor

An **actor** is the identity performing a transition. Actors are maps with `id` and `role` fields, specified with the `AS` clause:

```sql
TRANSITION SupportTicket "01J5..." TO resolved
  AS { id: "user-42", role: "admin" }
```

Guards reference the actor with the `ACTOR` keyword: `ACTOR.role == "admin"`.

## Composition

Machines can have **children** — instances of other machines that belong to a parent. This enables hierarchical patterns like Order → LineItem, Pipeline → Stage → Job.

```sql
CHILDREN {
  items    : LIST(LineItem) -> MIN(1)
  shipment : OPTIONAL(Shipment)
}
```

Children support predicates (`ALL()`, `ANY()`), cascade transitions, and `SIGNAL PARENT TO` for upward communication.
