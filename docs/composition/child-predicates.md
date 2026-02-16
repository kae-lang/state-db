# Child Predicates

Guards can evaluate conditions across all children in a slot using `ALL()` and `ANY()`. These predicates answer questions like "are all items confirmed?" or "has any stage failed?"

## ALL()

`ALL(children_name, condition)` returns `true` if every child in the slot satisfies the condition:

```sql
paid -> fulfilled {
  GUARD : ALL(items, STATE IS confirmed)
}
```

This transition only proceeds if every LineItem child in the `items` slot is in the `confirmed` state.

### Vacuous Truth

When the child list is empty, `ALL()` returns `true`. This follows the mathematical convention of vacuous truth -- "all zero items satisfy the condition" is trivially true.

```sql
-- If items is empty, this guard PASSES
GUARD : ALL(items, STATE IS confirmed)
```

::: warning
If your domain requires at least one child to exist, combine `ALL()` with a `MIN(1)` constraint on the CHILDREN declaration. The MIN constraint is checked at spawn time, not at guard evaluation time.
:::

If you need to guard against empty collections at transition time, add an explicit length check:

```sql
paid -> fulfilled {
  GUARD : len(items) > 0
  GUARD : ALL(items, STATE IS confirmed)
}
```

## ANY()

`ANY(children_name, condition)` returns `true` if at least one child satisfies the condition:

```sql
running -> failed {
  GUARD : ANY(stages, STATE IS failed)
}
```

This transition proceeds if any Stage child in the `stages` slot is in the `failed` state.

### Empty Collections

When the child list is empty, `ANY()` returns `false`. There is no child to satisfy the condition.

```sql
-- If stages is empty, this guard FAILS
GUARD : ANY(stages, STATE IS failed)
```

## Summary Table

| Predicate | Empty collection | At least one match | All match | None match |
|-----------|-----------------|-------------------|-----------|------------|
| `ALL()` | `true` | -- | `true` | `false` |
| `ANY()` | `false` | `true` | `true` | `false` |

## STATE IS Condition

The condition inside `ALL()` and `ANY()` uses `STATE IS` to check a child's current state:

```sql
GUARD : ALL(items, STATE IS confirmed)
GUARD : ANY(jobs, STATE IS running)
```

This checks the live current state of each child instance at the moment the guard is evaluated.

## Practical Examples

### Order Fulfillment

An order can only be fulfilled when every line item is confirmed:

```sql
DEFINE MACHINE Order (
  CHILDREN {
    items : LIST(LineItem) -> MIN(1)
  }
  TRANSITIONS {
    paid -> fulfilled {
      GUARD : ALL(items, STATE IS confirmed)
    }
  }
)
```

### Pipeline Failure Detection

A pipeline transitions to failed if any stage has failed:

```sql
DEFINE MACHINE Pipeline (
  CHILDREN {
    stages : LIST(Stage) -> MIN(1)
  }
  TRANSITIONS {
    running -> failed {
      GUARD : ANY(stages, STATE IS failed)
    }
    running -> completed {
      GUARD : ALL(stages, STATE IS passed)
    }
  }
)
```

### Shipment Readiness

An order can only transition to shipped when the single optional shipment is dispatched:

```sql
fulfilled -> shipped {
  GUARD : shipment.STATE IS dispatched
}
```

Note the dot syntax for OPTIONAL children: `shipment.STATE IS dispatched` checks the single Shipment child's state directly, rather than using a predicate over a list.

## Combining Predicates with Other Guards

Child predicates are regular guard expressions. They can be combined with other conditions:

```sql
paid -> fulfilled {
  GUARD : ALL(items, STATE IS confirmed)
  GUARD : total > 0
  GUARD : ACTOR.role IN ("admin", "fulfillment")
}
```

All guards must pass (they are ANDed together).

::: tip
Child predicates evaluate live state. If a child transitions between the time you check and the time the parent transitions, the guard re-evaluates at execution time. There is no stale read.
:::
