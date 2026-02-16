# Children Declaration

The `CHILDREN` block in a parent machine and the `PARENT` declaration in a child machine together establish a composition relationship.

## CHILDREN Block

Declared inside `DEFINE MACHINE`, the `CHILDREN` block names each child slot, its cardinality, and optional constraints:

```sql
DEFINE MACHINE Order (
  CHILDREN {
    items    : LIST(LineItem)      -> MIN(1)
    shipment : OPTIONAL(Shipment)
  }
  ...
)
```

### Cardinality Types

| Type | Meaning | Example |
|------|---------|---------|
| `LIST(Machine)` | Zero or more children of that type | `items : LIST(LineItem)` |
| `OPTIONAL(Machine)` | Zero or one child of that type | `shipment : OPTIONAL(Shipment)` |

### Constraints

Constraints follow the `->` arrow, just like data field constraints:

| Constraint | Applies to | Meaning |
|------------|------------|---------|
| `MIN(n)` | LIST | Minimum number of children required |
| `MAX(n)` | LIST | Maximum number of children allowed |

```sql
CHILDREN {
  items    : LIST(LineItem)    -> MIN(1), MAX(100)
  shipment : OPTIONAL(Shipment)
}
```

`OPTIONAL` implicitly means MIN(0), MAX(1) -- the engine enforces that at most one child of that type exists.

## PARENT Declaration

Every child machine must declare its parent type with `PARENT`:

```sql
DEFINE MACHINE LineItem (
  PARENT : Order
  DATA {
    product  : TEXT        -> REQUIRED
    quantity : INT         -> MIN(1), REQUIRED
    price    : MONEY(USD)  -> REQUIRED
  }
  STATES { pending, confirmed, backordered, cancelled }
  INITIAL STATE pending
  TERMINAL STATES { confirmed, cancelled }
  TRANSITIONS {
    pending -> confirmed { GUARD : quantity > 0 }
    pending -> backordered {}
    backordered -> confirmed {}
    ANY -> cancelled { EXCEPT FROM { confirmed } }
  }
)
```

The `PARENT` line must appear before the `DATA` block. The named machine must exist in the catalog and must declare a matching `CHILDREN` entry.

## The PARENT Keyword in Expressions

Inside a child machine's transitions, `PARENT` refers to the parent instance. You can access parent data fields:

```sql
pending -> backordered {
  ACTION : NOTIFY(PARENT.customer, "item.backordered")
}
```

Here, `PARENT.customer` resolves to the `customer` field on the parent Order instance.

## Internal Representation

Each child instance carries two internal fields:

| Field | Description |
|-------|-------------|
| `parent_id` | The ULID of the parent instance |
| `parent_machine` | The machine type of the parent (e.g., `"Order"`) |

These are set automatically when a child is spawned and cannot be changed. The engine maintains a parent index for fast lookups -- given a parent ID, it can efficiently find all children.

## Defining Both Sides

A valid composition requires both sides to be defined. The parent must list the child in its `CHILDREN` block, and the child must declare the `PARENT`:

```sql
-- Parent side
DEFINE MACHINE Pipeline (
  CHILDREN {
    stages : LIST(Stage) -> MIN(1)
  }
  ...
)

-- Child side
DEFINE MACHINE Stage (
  PARENT : Pipeline
  ...
)
```

::: warning
If you define a `PARENT` declaration pointing to a machine that does not have a matching `CHILDREN` entry, or vice versa, the engine will reject the definition.
:::

::: tip
Child machines can themselves have children, forming multi-level hierarchies. A Pipeline can own Stages, and each Stage can own Jobs. CASCADE and SIGNAL PARENT TO work recursively through the full tree.
:::
