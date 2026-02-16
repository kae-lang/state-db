# Order Processing with Composition

This guide walks through an e-commerce order system using SMQL's machine composition feature. Three machines -- Order, LineItem, and Shipment -- form a parent-child hierarchy. You will see how to spawn children, use composition predicates like `ALL()` and `ANY()` in guards, and use `CASCADE` transitions to propagate state changes through the tree.

## Prerequisites

Start the SMQL server:

```bash
smql-server --port 8080
```

---

## Architecture Overview

```
Order (parent)
  |
  +-- items: LIST(LineItem)      -- one or more line items
  +-- shipment: OPTIONAL(Shipment)  -- created at fulfillment
```

- **Order** tracks the overall order lifecycle from draft to delivery.
- **LineItem** represents individual products in the order. Each declares `PARENT: Order`.
- **Shipment** tracks the physical delivery. It also declares `PARENT: Order` and can `SIGNAL PARENT` when delivered.

---

## Step 1: Define All Three Machines

### Order

```sql
DEFINE MACHINE Order (
  DATA {
    customer : TEXT -> REQUIRED
    total    : INT  -> REQUIRED
    notes    : TEXT -> OPTIONAL
  }

  STATES { draft, placed, paid, fulfilled, shipped, delivered, cancelled, returned }
  INITIAL STATE draft
  TERMINAL STATES { delivered, cancelled, returned }

  CHILDREN {
    items    : LIST(LineItem)    -> MIN(1)
    shipment : OPTIONAL(Shipment)
  }

  TRANSITIONS {
    draft -> placed {
      GUARD : total > 0
    }
    placed -> paid {}
    paid -> fulfilled {
      GUARD : ALL(items, STATE IS confirmed)
    }
    fulfilled -> shipped {}
    shipped -> delivered {}
    delivered -> returned {}
    ANY -> cancelled {
      EXCEPT FROM { shipped, delivered, returned }
    }
  }
)
```

The key guard here is `ALL(items, STATE IS confirmed)` on the `paid -> fulfilled` transition. The order cannot be fulfilled until every line item has been confirmed.

### LineItem

```sql
DEFINE MACHINE LineItem (
  PARENT : Order

  DATA {
    product  : TEXT -> REQUIRED
    quantity : INT  -> REQUIRED
    price    : INT  -> REQUIRED
  }

  STATES { pending, confirmed, backordered, cancelled }
  INITIAL STATE pending
  TERMINAL STATES { confirmed, cancelled }

  TRANSITIONS {
    pending -> confirmed {
      GUARD : quantity > 0
    }
    pending -> backordered {}
    backordered -> confirmed {}
    ANY -> cancelled {
      EXCEPT FROM { confirmed }
    }
  }
)
```

Notice the `PARENT : Order` declaration. This links every LineItem instance back to its parent Order. The `EXCEPT FROM { confirmed }` on the cancellation wildcard prevents confirmed items from being cancelled.

### Shipment

```sql
DEFINE MACHINE Shipment (
  PARENT : Order

  DATA {
    tracking : TEXT -> OPTIONAL
    carrier  : TEXT -> OPTIONAL
  }

  STATES { created, dispatched, in_transit, delivered, lost }
  INITIAL STATE created
  TERMINAL STATES { delivered, lost }

  TRANSITIONS {
    created -> dispatched {
      GUARD : tracking IS SET
      GUARD : carrier IS SET
    }
    dispatched -> in_transit {}
    in_transit -> delivered {
      SIGNAL PARENT TO delivered
    }
    in_transit -> lost {}
  }
)
```

The `SIGNAL PARENT TO delivered` on the `in_transit -> delivered` transition means that when the shipment is delivered, it signals the parent Order to transition to its `delivered` state.

### Register All Machines

Send each definition to the server. The order matters -- child machines reference parent machines, so register the parent first:

```bash
# Register Order first (parent)
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "DEFINE MACHINE Order ( DATA { customer: TEXT -> REQUIRED, total: INT -> REQUIRED, notes: TEXT -> OPTIONAL } STATES { draft, placed, paid, fulfilled, shipped, delivered, cancelled, returned } INITIAL STATE draft TERMINAL STATES { delivered, cancelled, returned } CHILDREN { items: LIST(LineItem) -> MIN(1), shipment: OPTIONAL(Shipment) } TRANSITIONS { draft -> placed { GUARD: total > 0 } placed -> paid {} paid -> fulfilled { GUARD: ALL(items, STATE IS confirmed) } fulfilled -> shipped {} shipped -> delivered {} delivered -> returned {} ANY -> cancelled { EXCEPT FROM { shipped, delivered, returned } } } )"}'

# Register LineItem
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "DEFINE MACHINE LineItem ( PARENT: Order DATA { product: TEXT -> REQUIRED, quantity: INT -> REQUIRED, price: INT -> REQUIRED } STATES { pending, confirmed, backordered, cancelled } INITIAL STATE pending TERMINAL STATES { confirmed, cancelled } TRANSITIONS { pending -> confirmed { GUARD: quantity > 0 } pending -> backordered {} backordered -> confirmed {} ANY -> cancelled { EXCEPT FROM { confirmed } } } )"}'

# Register Shipment
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "DEFINE MACHINE Shipment ( PARENT: Order DATA { tracking: TEXT -> OPTIONAL, carrier: TEXT -> OPTIONAL } STATES { created, dispatched, in_transit, delivered, lost } INITIAL STATE created TERMINAL STATES { delivered, lost } TRANSITIONS { created -> dispatched { GUARD: tracking IS SET GUARD: carrier IS SET } dispatched -> in_transit {} in_transit -> delivered {} in_transit -> lost {} } )"}'
```

---

## Step 2: Create an Order with Line Items

### Spawn the Order

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "SPAWN Order { customer: \"customer_001\", total: 9999 }"
  }'
```

```json
{
  "success": true,
  "result": {
    "id": "01JMORDER0000000000000001",
    "machine": "Order",
    "state": "draft",
    "data": {
      "customer": "customer_001",
      "total": 9999
    },
    "trail_length": 1,
    "version": 1
  }
}
```

Save the order ID -- you will need it to spawn child items.

### Spawn Line Items as Children

Use the `PARENT` clause in the spawn command to link children to the order:

```bash
# Item 1: Widget A, qty 2, $25.00
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "SPAWN LineItem { product: \"Widget A\", quantity: 2, price: 2500 } PARENT Order \"01JMORDER0000000000000001\""
  }'
```

```json
{
  "success": true,
  "result": {
    "id": "01JMITEM00000000000000001",
    "machine": "LineItem",
    "state": "pending",
    "data": {
      "product": "Widget A",
      "quantity": 2,
      "price": 2500
    },
    "trail_length": 1,
    "version": 1
  }
}
```

```bash
# Item 2: Widget B, qty 1, $49.99
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "SPAWN LineItem { product: \"Widget B\", quantity: 1, price: 4999 } PARENT Order \"01JMORDER0000000000000001\""
  }'
```

Both items start in the `pending` state and are linked to the parent order.

---

## Step 3: Place the Order

The `draft -> placed` guard requires `total > 0`:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION \"01JMORDER0000000000000001\" TO placed"
  }'
```

```json
{
  "success": true,
  "result": {
    "from_state": "draft",
    "to_state": "placed",
    "instance": { "state": "placed", "version": 2 }
  }
}
```

### Guard Failure: Zero Total

An order with `total: 0` would be rejected:

```json
{
  "success": false,
  "error": "Transition denied: guard failed: total > 0"
}
```

---

## Step 4: Process Payment

Move to `paid`. In a production system, this transition would be triggered by a payment signal:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION \"01JMORDER0000000000000001\" TO paid"
  }'
```

---

## Step 5: Confirm Line Items

Before the order can be fulfilled, every line item must be confirmed. This is enforced by the `ALL()` composition predicate.

### Confirm First Item

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION \"01JMITEM00000000000000001\" TO confirmed"
  }'
```

### Try to Fulfill -- Blocked by ALL()

With only one of two items confirmed, the `ALL(items, STATE IS confirmed)` guard fails:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION \"01JMORDER0000000000000001\" TO fulfilled"
  }'
```

```json
{
  "success": false,
  "error": "Transition denied: guard failed: ALL(items, STATE IS confirmed)"
}
```

### Confirm Second Item

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION \"01JMITEM00000000000000002\" TO confirmed"
  }'
```

### Fulfill Successfully

Now that all items are confirmed, the guard passes:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION \"01JMORDER0000000000000001\" TO fulfilled"
  }'
```

```json
{
  "success": true,
  "result": {
    "from_state": "paid",
    "to_state": "fulfilled"
  }
}
```

---

## Step 6: Ship and Deliver

### Spawn a Shipment

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "SPAWN Shipment {} PARENT Order \"01JMORDER0000000000000001\""
  }'
```

```json
{
  "success": true,
  "result": {
    "id": "01JMSHIP00000000000000001",
    "machine": "Shipment",
    "state": "created"
  }
}
```

### Dispatch the Shipment

Both `tracking IS SET` and `carrier IS SET` guards must pass:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION \"01JMSHIP00000000000000001\" TO dispatched WITH { tracking: \"TRACK123456\", carrier: \"fedex\" }"
  }'
```

Without tracking information, the guard rejects the transition:

```json
{
  "success": false,
  "error": "Transition denied: guard failed: tracking IS SET"
}
```

### Move Through Transit to Delivery

```bash
# dispatched -> in_transit
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION \"01JMSHIP00000000000000001\" TO in_transit"
  }'

# in_transit -> delivered (signals parent Order)
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION \"01JMSHIP00000000000000001\" TO delivered"
  }'
```

The `SIGNAL PARENT TO delivered` declaration means the shipment's delivery automatically signals the parent Order to transition to `delivered`.

### Complete the Order

After shipping and delivery signals:

```bash
# Ship the order
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION \"01JMORDER0000000000000001\" TO shipped"
  }'

# Deliver the order
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION \"01JMORDER0000000000000001\" TO delivered"
  }'
```

---

## CASCADE Cancellation

The `ANY -> cancelled` wildcard on Order allows cancellation from most states. Using `CASCADE`, you can cancel the order and propagate the cancellation to all children:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION \"01JMORDER0000000000000001\" TO cancelled CASCADE"
  }'
```

CASCADE works as follows:

1. The parent order transitions to `cancelled`.
2. For each child, SMQL attempts a `try_transition` to the first terminal state of that child machine.
3. For LineItem, terminal states are `{ confirmed, cancelled }`. CASCADE tries `confirmed` first -- but if the item is still `pending`, the `quantity > 0` guard may or may not pass. Then it tries `cancelled`.
4. Children already in a terminal state are left untouched.

**Important:** CASCADE uses `try_transition` (best-effort). If a child's guard blocks the transition to its first terminal state, CASCADE is not guaranteed to move every child. In practice, design child machines with a `cancelled` terminal state reachable from all non-terminal states.

The `EXCEPT FROM { confirmed }` on LineItem's cancellation wildcard means confirmed items cannot be cancelled, even by CASCADE. This protects confirmed items from accidental rollback.

---

## Querying Across the Hierarchy

### Find Pending Line Items

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "FIND LineItem WHERE STATE IS pending"
  }'
```

```json
{
  "success": true,
  "result": {
    "count": 1,
    "instances": [
      { "id": "01JMITEM...", "state": "pending", "data": { "product": "Widget B" } }
    ]
  }
}
```

### Aggregate Line Items by State

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "AGGREGATE LineItem MEASURE COUNT() GROUP BY state"
  }'
```

```json
{
  "success": true,
  "result": {
    "rows": [
      { "group": { "state": "pending" }, "measures": { "count": 2 } },
      { "group": { "state": "confirmed" }, "measures": { "count": 1 } }
    ]
  }
}
```

### View Order Trail

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRAIL OF Order \"01JMORDER0000000000000001\""
  }'
```

```json
{
  "success": true,
  "result": {
    "count": 6,
    "entries": [
      { "sequence": 0, "from_state": "", "to_state": "draft", "timestamp": "2026-02-16T10:00:00Z" },
      { "sequence": 1, "from_state": "draft", "to_state": "placed", "timestamp": "2026-02-16T10:01:00Z" },
      { "sequence": 2, "from_state": "placed", "to_state": "paid", "timestamp": "2026-02-16T10:02:00Z" },
      { "sequence": 3, "from_state": "paid", "to_state": "fulfilled", "timestamp": "2026-02-16T10:10:00Z" },
      { "sequence": 4, "from_state": "fulfilled", "to_state": "shipped", "timestamp": "2026-02-16T10:30:00Z" },
      { "sequence": 5, "from_state": "shipped", "to_state": "delivered", "timestamp": "2026-02-16T12:00:00Z" }
    ]
  }
}
```

---

## Composition Predicates Reference

| Predicate | Meaning | Example |
|---|---|---|
| `ALL(children_ref, condition)` | Every child must satisfy the condition | `ALL(items, STATE IS confirmed)` |
| `ANY(children_ref, condition)` | At least one child satisfies the condition | `ANY(items, STATE IS backordered)` |
| `ALL()` over empty children | Returns `true` (vacuous truth) | An order with no items passes `ALL(items, ...)` |
| `ANY()` over empty children | Returns `false` | An order with no items fails `ANY(items, ...)` |

These predicates query the live state of child instances at guard evaluation time. They are not cached -- each transition re-evaluates the predicate against the current state of all children.
