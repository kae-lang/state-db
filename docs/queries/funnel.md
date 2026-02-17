# FUNNEL

Measure how many instances have visited each state in an ordered sequence. Useful for analyzing conversion pipelines and drop-off rates.

## Syntax

```smql
FUNNEL MachineName THROUGH [state1, state2, state3, ...] [WHERE <predicate>]
```

The states are listed inside square brackets `[...]`, separated by commas.

## Examples

```smql
-- Full conversion funnel
FUNNEL SupportTicket THROUGH [open, triaged, in_progress, resolved, closed]

-- Funnel with filter
FUNNEL Order THROUGH [draft, placed, paid, shipped, delivered]
  WHERE created_at > "2025-01-01"

-- Partial funnel (subset of states)
FUNNEL SupportTicket THROUGH [open, resolved]
```

## How It Works

For each state in the sequence, SMQL counts how many of the matching instances have **ever visited** that state (based on the current state or trail history). An instance counts toward a stage if it is currently in that state or has a trail entry showing it transitioned to that state at some point.

The `conversion_rate` is computed as the ratio of instances that reached this state compared to the **total number of matching instances** (not compared to the previous stage).

## Response

```json
{
  "success": true,
  "result": {
    "stages": [
      {
        "state": "open",
        "count": 100,
        "conversion_rate": 1.0
      },
      {
        "state": "triaged",
        "count": 85,
        "conversion_rate": 0.85
      },
      {
        "state": "in_progress",
        "count": 80,
        "conversion_rate": 0.8
      },
      {
        "state": "resolved",
        "count": 72,
        "conversion_rate": 0.72
      },
      {
        "state": "closed",
        "count": 70,
        "conversion_rate": 0.7
      }
    ]
  }
}
```

::: info
The `conversion_rate` is relative to the total instance count, not the previous stage. For example, if 100 instances exist and 72 reached `resolved`, `conversion_rate` = 0.72 regardless of how many reached `in_progress`. To calculate stage-to-stage drop-off, divide adjacent counts.
:::

When no instances match (empty machine or all filtered out), all stages return `count: 0` and `conversion_rate: 0.0`.

## SDK

```rust
let result = client
    .execute("FUNNEL SupportTicket THROUGH [open, triaged, resolved, closed]")
    .await?;
```
