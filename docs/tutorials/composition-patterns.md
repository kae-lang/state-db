# Tutorial 4: Composition Patterns

In the [previous tutorial](./timeouts-and-hooks), you added timeouts and hooks. But so far, each machine is independent. Real systems have hierarchies — an order contains line items, a pipeline has stages, a project has tasks.

SMQL supports this natively through **composition**: machines that own instances of other machines.

## What You'll Build

An `Invoice` machine with `LineItem` children. The invoice can only be sent once all line items are confirmed, and canceling the invoice cascades to all children.

```
Invoice (parent)
  ├── LineItem #1 (child)
  ├── LineItem #2 (child)
  └── LineItem #3 (child)

Invoice: draft ──→ sent ──→ paid ──→ archived
LineItem: pending ──→ confirmed | cancelled
```

## Step 1: Define the Parent Machine

The parent machine declares its children with the `CHILDREN` block:

```sql
DEFINE MACHINE Invoice (

  DATA {
    customer : TEXT       -> REQUIRED
    due_date : TEXT       -> OPTIONAL
    total    : INT        -> REQUIRED
  }

  STATES { draft, sent, paid, cancelled, archived }
  INITIAL STATE draft
  TERMINAL STATES { archived, cancelled }

  CHILDREN {
    items : LIST(LineItem) -> MIN(1)
  }

  TRANSITIONS {
    draft -> sent {
      GUARD : ALL(items, STATE IS confirmed)
      GUARD : total > 0
    }

    sent -> paid {}

    paid -> archived {}

    ANY -> cancelled {
      EXCEPT FROM { paid, archived }
    }
  }
)
```

### CHILDREN Declaration

```sql
CHILDREN {
  items : LIST(LineItem) -> MIN(1)
}
```

This declares:
- `items` — the relationship name, used in guards and queries
- `LIST(LineItem)` — a list of `LineItem` instances
- `MIN(1)` — at least one child is required before certain operations

You can also declare optional children:

```sql
CHILDREN {
  items    : LIST(LineItem)       -> MIN(1)
  shipment : OPTIONAL(Shipment)
}
```

### The ALL Predicate

```sql
GUARD : ALL(items, STATE IS confirmed)
```

This guard checks that **every** child in the `items` relationship is in the `confirmed` state. If any child is in a different state, the guard fails.

::: info
`ALL` over an empty collection returns **true** (vacuous truth). `ANY` over an empty collection returns **false**. This matches standard logic.
:::

## Step 2: Define the Child Machine

The child machine declares its parent with the `PARENT` keyword:

```sql
DEFINE MACHINE LineItem (

  PARENT : Invoice

  DATA {
    product  : TEXT -> REQUIRED
    quantity : INT  -> MIN(1), REQUIRED
    price    : INT  -> REQUIRED
  }

  STATES { pending, confirmed, cancelled }
  INITIAL STATE pending
  TERMINAL STATES { confirmed, cancelled }

  TRANSITIONS {
    pending -> confirmed {
      GUARD : quantity > 0
    }

    pending -> cancelled {}
  }
)
```

::: warning
Register the parent machine **before** the child machine. The child's `PARENT : Invoice` must reference an already-defined machine.
:::

## Step 3: Build the Hierarchy

### Create the Parent

```bash
> SPAWN Invoice { customer: "Acme Corp", total: 7500 }
```

```json
{
  "result": {
    "id": "01JMINV000000000000000001A",
    "machine": "Invoice",
    "state": "draft",
    "data": { "customer": "Acme Corp", "total": 7500 }
  }
}
```

### Create Children

When spawning a child, provide the parent ID and machine:

::: code-group
```bash [REPL]
> SPAWN LineItem { product: "Widget A", quantity: 3, price: 2500 } PARENT "01JMINV000000000000000001A"
> SPAWN LineItem { product: "Widget B", quantity: 1, price: 5000 } PARENT "01JMINV000000000000000001A"
```

```bash [curl]
curl -X POST http://localhost:4200/execute \
  -H 'Content-Type: application/json' \
  -d '{
    "smql": "SPAWN LineItem { product: \"Widget A\", quantity: 3, price: 2500 } PARENT \"01JMINV000000000000000001A\""
  }'
```
:::

```json
{
  "result": {
    "id": "01JMLI0000000000000000001A",
    "machine": "LineItem",
    "state": "pending",
    "data": { "product": "Widget A", "quantity": 3, "price": 2500 },
    "parent_id": "01JMINV000000000000000001A",
    "parent_machine": "Invoice"
  }
}
```

Each child instance has `parent_id` and `parent_machine` fields linking it to the parent.

## Step 4: ALL/ANY Guards in Action

### Try to Send Before Children Are Confirmed

```bash
> TRANSITION Invoice "01JMINV000000000000000001A" TO sent
```

```json
{
  "success": false,
  "error": "Transition denied: guard failed: ALL(items, STATE IS confirmed)"
}
```

The `ALL(items, STATE IS confirmed)` guard fails because both line items are still in `pending`.

### Confirm the Children

```bash
> TRANSITION LineItem "01JMLI0000000000000000001A" TO confirmed
> TRANSITION LineItem "01JMLI0000000000000000002A" TO confirmed
```

### Now Send the Invoice

```bash
> TRANSITION Invoice "01JMINV000000000000000001A" TO sent
```

```json
{
  "success": true,
  "result": { "from_state": "draft", "to_state": "sent" }
}
```

Both guards pass now:
1. `ALL(items, STATE IS confirmed)` — both children are confirmed
2. `total > 0` — total is 7500

### The ANY Predicate

`ANY` works like `ALL` but requires only **one** child to match:

```sql
-- Fails if ANY line item is cancelled
GUARD : ANY(items, STATE IS cancelled) == false

-- Proceeds if ANY line item is ready
running -> passed {
  GUARD : ANY(items, STATE IS confirmed)
}
```

## Step 5: CASCADE Transitions

When you cancel an invoice, you probably want to cancel all its line items too. The `CASCADE` modifier does this:

```bash
> SPAWN Invoice { customer: "Beta Corp", total: 3000 }
> SPAWN LineItem { product: "Gadget", quantity: 1, price: 3000 } PARENT "<invoice_id>"
> TRANSITION Invoice "<invoice_id>" TO cancelled CASCADE
```

CASCADE behavior:
1. The parent transitions to `cancelled`
2. Each child **attempts** to transition to its first terminal state
3. For `LineItem`, terminal states are `[confirmed, cancelled]` — it tries `confirmed` first
4. If that transition's guards fail, it tries the next terminal state (`cancelled`)
5. This is **best-effort** — child failures don't block the parent

::: tip
CASCADE tries terminal states in the order they appear in the `TERMINAL STATES` declaration. Put the "default" terminal state first.
:::

## Step 6: Querying Children

### Find All Children of a Parent

```bash
> FIND LineItem WHERE parent == "01JMINV000000000000000001A"
```

### Get Parent from Child

When you `GET` a child instance, the response includes parent information:

```bash
> GET LineItem "01JMLI0000000000000000001A"
```

```json
{
  "result": {
    "id": "01JMLI0000000000000000001A",
    "machine": "LineItem",
    "state": "confirmed",
    "parent_id": "01JMINV000000000000000001A",
    "parent_machine": "Invoice"
  }
}
```

## Multi-Level Composition

SMQL supports composition beyond two levels. A common pattern is three-level hierarchies:

```sql
DEFINE MACHINE Pipeline (
  CHILDREN { stages : LIST(Stage) -> MIN(1) }
  TRANSITIONS {
    running -> passed { GUARD : ALL(stages, STATE IS passed) }
  }
)

DEFINE MACHINE Stage (
  PARENT : Pipeline
  CHILDREN { jobs : LIST(Job) -> MIN(1) }
  TRANSITIONS {
    running -> passed { GUARD : ALL(jobs, STATE IS passed) }
  }
)

DEFINE MACHINE Job (
  PARENT : Stage
  TRANSITIONS {
    running -> passed {}
    running -> failed {}
  }
)
```

In this pattern, the Pipeline can only pass when ALL stages pass, and each Stage can only pass when ALL its jobs pass. Changes propagate up the hierarchy through guard evaluation.

## What You Learned

| Concept | Summary |
|---------|---------|
| `CHILDREN` block | Declares child relationships with type and cardinality |
| `PARENT : Machine` | Links a child machine to its parent type |
| `LIST(Machine)` | A collection of child instances |
| `OPTIONAL(Machine)` | A single optional child instance |
| `MIN(n)` | Minimum cardinality constraint |
| `ALL(children, predicate)` | Guard that requires all children to match |
| `ANY(children, predicate)` | Guard that requires at least one child to match |
| `CASCADE` | Recursively transitions children to terminal states |
| Multi-level | Composition can nest arbitrarily deep |

## Next Step

You now know how to build machine hierarchies. But how do you analyze what's happening across hundreds or thousands of instances? In the [next tutorial](./queries-and-analytics), you'll learn SMQL's powerful query and analytics capabilities.
