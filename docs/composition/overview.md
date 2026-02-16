# Composition Overview

Composition lets machines own other machines. An Order owns LineItems. A Pipeline owns Stages. A CI Build owns Jobs. These are parent-child relationships, and SMQL models them as first-class concepts.

## Why Composition?

A simple `REF(Order)` field on a LineItem establishes a loose pointer -- nothing more. The engine has no knowledge of the relationship. It cannot enforce constraints, propagate transitions, or answer queries about children.

Composition goes further:

| Capability | REF field | CHILDREN block |
|------------|-----------|----------------|
| Engine-managed relationship | No | Yes |
| `ALL()` / `ANY()` predicates in guards | No | Yes |
| CASCADE transitions | No | Yes |
| SIGNAL PARENT TO | No | Yes |
| MIN/MAX cardinality constraints | No | Yes |
| Parent-scoped queries | No | Yes |

## The Model

A composed system has two sides:

**Parent machine** -- declares a `CHILDREN` block listing named child slots:

```sql
DEFINE MACHINE Order (
  CHILDREN {
    items    : LIST(LineItem)    -> MIN(1)
    shipment : OPTIONAL(Shipment)
  }
  ...
)
```

**Child machine** -- declares a `PARENT` line naming the parent type:

```sql
DEFINE MACHINE LineItem (
  PARENT : Order
  ...
)
```

Every child instance carries a `parent_id` and `parent_machine` internally. The engine uses these to maintain a parent index for efficient child lookups.

## How It Differs from Flat Machines

Without composition, you would need to:

1. Manually query for related instances before each transition
2. Write application-level code to enforce "all items must be confirmed before fulfillment"
3. Implement your own cascade logic when cancelling an order
4. Build custom event plumbing so a child can trigger a parent transition

With composition, these are declarative:

```sql
-- Guard on children's state
paid -> fulfilled {
  GUARD : ALL(items, STATE IS confirmed)
}

-- Cancel order and all children
TRANSITION "01J5..." TO cancelled CASCADE

-- Child signals parent
in_transit -> delivered {
  SIGNAL PARENT TO delivered
}
```

## A Complete Example

Here is an Order system with two child machine types:

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
    draft -> placed {}
    placed -> paid {}
    paid -> fulfilled {
      GUARD  : ALL(items, STATE IS confirmed)
      MUTATE : shipment = SPAWN Shipment { order: SELF }
    }
    fulfilled -> shipped {
      GUARD : shipment.STATE IS dispatched
    }
    shipped -> delivered {}
    ANY -> cancelled {
      EXCEPT FROM { shipped, delivered, returned }
    }
  }
)
```

The following pages cover each aspect in detail:

- [Children Declaration](./children-declaration) -- CHILDREN block syntax and PARENT declarations
- [Spawning Children](./spawning-children) -- creating child instances with SPAWN
- [Child Predicates](./child-predicates) -- ALL() and ANY() guards over children
- [Signal Parent](./signal-parent) -- upward communication with SIGNAL PARENT TO
- [Cascade](./cascade) -- propagating transitions to children with CASCADE

::: tip
Composition is optional. Most machines are standalone. Only add a CHILDREN block when your domain genuinely has ownership semantics -- an order that owns its line items, a pipeline that owns its stages.
:::
