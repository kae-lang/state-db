# Spawning Children

Children are instances that belong to a parent. They can be created in two ways: via SPAWN in a MUTATE clause during a transition, or externally with a standalone SPAWN command.

## SPAWN in MUTATE

The most common pattern is spawning a child as part of a parent's transition. The `MUTATE` clause assigns the spawned instance to a child slot:

```sql
paid -> fulfilled {
  GUARD  : ALL(items, STATE IS confirmed)
  MUTATE : shipment = SPAWN Shipment { order: SELF }
}
```

When this transition fires:

1. A new Shipment instance is created in its initial state (`created`)
2. The Shipment's `parent_id` is set to the Order's ID
3. The Shipment's `parent_machine` is set to `"Order"`
4. The Shipment's `order` data field is set to the Order instance (via `SELF`)
5. The `shipment` child slot on the Order now references this instance

### SELF Keyword

`SELF` refers to the current (parent) instance. It is commonly passed to the child so the child has a reference back:

```sql
MUTATE : shipment = SPAWN Shipment { order: SELF }
```

### Empty Data Blocks

SPAWN always requires braces, even when passing no initial data:

```sql
MUTATE : tracker = SPAWN Tracker {}
```

::: warning
`SPAWN Tracker` without `{}` is a syntax error. Always include the braces.
:::

### Multiple Spawns

A single transition can spawn multiple children:

```sql
approved -> active {
  MUTATE : primary = SPAWN Account { type: "primary", owner: SELF }
  MUTATE : savings = SPAWN Account { type: "savings", owner: SELF }
}
```

## External SPAWN

Children can also be spawned outside of a transition using the standard SPAWN command. The engine associates the child with its parent based on the machine definitions:

```sql
SPAWN LineItem {
  product: "Widget",
  quantity: 3,
  price: 9.99
}
```

When spawning a child externally, the parent ID must be provided so the engine can establish the relationship. The server API accepts the parent context:

::: code-group
```bash [curl]
curl -X POST http://localhost:4200/execute \
  -H 'Content-Type: application/json' \
  -d '{
    "smql": "SPAWN LineItem { product: \"Widget\", quantity: 3, price: 9.99 }",
    "parent_id": "01J5X7K2P3Q4R5S6T7U8V9W0XY"
  }'
```

```bash [REPL]
> SPAWN LineItem { product: "Widget", quantity: 3, price: 9.99 }
```
:::

## Spawning into LIST vs OPTIONAL

The cardinality type determines how many children can exist:

### LIST Children

A `LIST` slot accepts multiple children. Each SPAWN adds to the list:

```sql
CHILDREN {
  items : LIST(LineItem) -> MIN(1)
}
```

You can spawn as many LineItem children as needed (subject to MAX constraints if defined).

### OPTIONAL Children

An `OPTIONAL` slot accepts at most one child:

```sql
CHILDREN {
  shipment : OPTIONAL(Shipment)
}
```

If a Shipment already exists for this Order, attempting to spawn another will fail.

## Spawn Validation

When spawning a child, the engine validates:

1. **Parent exists** -- the parent instance must exist and not be in a terminal state
2. **Machine match** -- the child machine must be declared in the parent's CHILDREN block
3. **Cardinality** -- LIST respects MIN/MAX; OPTIONAL allows at most one
4. **Data constraints** -- all REQUIRED fields present, types match, constraints satisfied
5. **PARENT declaration** -- the child machine must declare the correct PARENT type

## Lifecycle

Spawned children begin in their initial state, just like any other instance. They receive a trail entry at sequence 0 recording the spawn event. From that point, they transition independently -- unless the parent uses [CASCADE](./cascade) or the child uses [SIGNAL PARENT TO](./signal-parent).

::: tip
Children are independent instances with their own state, data, version, and trail. Composition provides the relationship and coordination primitives, but each child has its own lifecycle.
:::
