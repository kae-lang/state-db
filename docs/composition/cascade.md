# Cascade

`CASCADE` propagates a transition downward through the composition tree. When a parent transitions with CASCADE, all of its children are recursively transitioned to a terminal state.

## Syntax

Append `CASCADE` to a TRANSITION command:

```sql
TRANSITION Order "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO cancelled CASCADE
```

## How It Works

When a transition includes CASCADE, the engine:

1. Transitions the parent instance to the target state (guards, mutations, and actions execute as normal)
2. Finds all children of the parent instance
3. For each child that is not already in a terminal state, attempts to transition it to its **first declared terminal state**
4. Recursively repeats step 2-3 for each child's children

## First Terminal State

CASCADE transitions each child to the **first** terminal state declared in the child machine's definition. The ordering in the `TERMINAL STATES` block determines which state is chosen:

```sql
DEFINE MACHINE LineItem (
  TERMINAL STATES { confirmed, cancelled }
  ...
)
```

When an Order is cancelled with CASCADE, each LineItem child will be transitioned to `confirmed` -- the first terminal state. If you want cancellation to propagate as cancellation, put `cancelled` first:

```sql
DEFINE MACHINE LineItem (
  TERMINAL STATES { cancelled, confirmed }
  ...
)
```

Now CASCADE will attempt to transition each LineItem to `cancelled`.

::: warning
The order of terminal states matters for CASCADE. Put the "default shutdown" state first if you want CASCADE to use it. In most designs, `cancelled` should be listed first in TERMINAL STATES when the machine participates in a CASCADE cancellation pattern.
:::

## Guard Behavior

CASCADE still evaluates guards on each child transition. If the first terminal state's transition has a guard that fails, that child remains in its current state. CASCADE does not try the second terminal state.

```sql
DEFINE MACHINE LineItem (
  TERMINAL STATES { cancelled, confirmed }
  TRANSITIONS {
    ANY -> cancelled {
      EXCEPT FROM { confirmed }
      GUARD : ACTOR.role == "admin"
    }
  }
)
```

If CASCADE triggers without an actor (which is typical), the guard `ACTOR.role == "admin"` will fail, and the LineItem stays in its current state.

::: warning
CASCADE only attempts the **first** terminal state. If its guard fails, the child stays where it is. Design your terminal transitions with this in mind -- guards on cascade-targeted transitions should be permissive, or the cascade will silently leave children behind.
:::

## Recursive Depth

CASCADE is recursive. If a child has its own children, those grandchildren are also cascaded:

```
Order (cancelled via CASCADE)
  |-- LineItem_1 -> cancelled
  |-- LineItem_2 -> cancelled
  |-- Shipment -> lost
        |-- TrackingEvent_1 -> archived
        |-- TrackingEvent_2 -> archived
```

Each level independently resolves to the first terminal state of that machine type.

## Example: Cancelling an Order

```sql
DEFINE MACHINE Order (
  DATA {
    customer : REF(Customer) -> REQUIRED
    total    : MONEY(USD)    -> REQUIRED
    notes    : TEXT           -> OPTIONAL
  }
  STATES { draft, placed, paid, payment_failed, fulfilled, shipped, delivered, cancelled, returned }
  INITIAL STATE draft
  TERMINAL STATES { delivered, cancelled, returned }
  CHILDREN {
    items    : LIST(LineItem)      -> MIN(1)
    shipment : OPTIONAL(Shipment)
  }
  TRANSITIONS {
    ANY -> cancelled {
      EXCEPT FROM { shipped, delivered, returned }
    }
  }
)
```

Cancelling with CASCADE:

```sql
TRANSITION Order "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO cancelled CASCADE
```

This will:

1. Transition the Order to `cancelled`
2. Find all LineItem children -- transition each to their first terminal state
3. Find the Shipment child (if any) -- transition it to its first terminal state
4. If the Shipment has children, cascade to them as well

## CASCADE with Other Clauses

CASCADE can be combined with other transition clauses:

```sql
TRANSITION Order "01J5..." TO cancelled CASCADE
  AS { id: "admin-1", role: "admin" }
  WITH { cancellation_reason: "Customer request" }
  MEMO "Cancelled per customer phone call"
```

The AS, WITH, and MEMO clauses apply to the parent transition only. Child transitions triggered by CASCADE execute without actor, data, or memo context.

## When Not to CASCADE

CASCADE is a blunt instrument. It is best suited for shutdown scenarios:

- Cancelling an order and all its line items
- Aborting a pipeline and all its stages
- Archiving a project and all its tasks

For more nuanced scenarios where children should transition to different states based on their current state, use explicit transitions on each child instead of CASCADE.

::: tip
CASCADE is a command-time option, not a machine definition feature. The machine does not declare "this transition always cascades." The caller decides whether to include CASCADE when issuing the TRANSITION command.
:::
