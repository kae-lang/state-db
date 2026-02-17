# COMPARE PATHS

Analyze transition paths segmented by a data field. This lets you compare how different categories of instances flow through states — for example, how high-priority tickets take different paths than low-priority ones.

## Syntax

```smql
COMPARE PATHS MachineName SEGMENT BY field_name [WHERE <predicate>]
```

## Examples

```smql
-- Compare paths by priority level
COMPARE PATHS SupportTicket SEGMENT BY priority

-- Compare paths by category with a filter
COMPARE PATHS SupportTicket SEGMENT BY category
  WHERE created_at > "2025-01-01"

-- Compare order paths by payment method
COMPARE PATHS Order SEGMENT BY payment_method
```

## How It Works

1. All instances of the machine are fetched (filtered by WHERE if provided)
2. Instances are grouped by the value of the `SEGMENT BY` field
3. Within each group, paths are analyzed the same way as [PATHS](./paths) — built from the trail and counted by unique sequence
4. Segments are sorted by total instance count (descending)

## Response

```json
{
  "success": true,
  "result": {
    "segment_by": "priority",
    "segments": [
      {
        "segment_value": "high",
        "paths": [
          {
            "path": ["", "open", "triaged", "in_progress", "resolved", "closed"],
            "count": 25
          },
          {
            "path": ["", "open", "triaged", "in_progress", "escalated", "resolved", "closed"],
            "count": 8
          }
        ]
      },
      {
        "segment_value": "medium",
        "paths": [
          {
            "path": ["", "open", "triaged", "in_progress", "resolved", "closed"],
            "count": 40
          },
          {
            "path": ["", "open", "cancelled"],
            "count": 3
          }
        ]
      },
      {
        "segment_value": "low",
        "paths": [
          {
            "path": ["", "open", "triaged", "resolved", "closed"],
            "count": 15
          },
          {
            "path": ["", "open", "cancelled"],
            "count": 10
          }
        ]
      }
    ]
  }
}
```

Each segment contains:

- `segment_value` — the value of the SEGMENT BY field for this group
- `paths` — an array of distinct paths with counts, sorted by count descending (same format as [PATHS](./paths))

::: tip
COMPARE PATHS is useful for identifying how different instance categories take different routes through the state machine. For example, you might discover that "critical" tickets skip the triage step while "low" tickets are more likely to be cancelled.
:::

## SDK

```rust
let result = client
    .execute("COMPARE PATHS SupportTicket SEGMENT BY priority")
    .await?;
```
