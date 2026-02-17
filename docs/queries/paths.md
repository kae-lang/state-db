# PATHS

Analyze the state transition paths taken by instances of a machine. Returns distinct state sequences with counts, sorted by frequency.

## Syntax

```smql
PATHS FROM MachineName [WHERE <predicate>] [LIMIT n]
```

## Examples

```smql
-- All paths for a machine
PATHS FROM SupportTicket

-- Top 5 most common paths
PATHS FROM SupportTicket LIMIT 5

-- Paths filtered by data
PATHS FROM SupportTicket WHERE priority == "critical"
```

## How Paths Are Built

Each instance's path is constructed from its trail (audit log):

1. The first element is the spawn entry's `from_state` (an empty string `""`)
2. Each subsequent element is the `to_state` of each trail entry

For example, an instance spawned in `open` that transitions through `in_progress` → `review` → `done` produces the path: `["", "open", "in_progress", "review", "done"]`.

Paths are grouped by exact sequence. Two instances with identical state sequences share the same path entry with a combined count.

## Response

Results are sorted by `count` descending (most common paths first):

```json
{
  "success": true,
  "result": {
    "paths": [
      {
        "path": ["", "open", "triaged", "in_progress", "resolved", "closed"],
        "count": 35
      },
      {
        "path": ["", "open", "triaged", "in_progress", "resolved", "reopened", "in_progress", "resolved", "closed"],
        "count": 12
      },
      {
        "path": ["", "open", "cancelled"],
        "count": 5
      }
    ]
  }
}
```

::: info
All instances are included in path analysis, not just those in terminal states. In-progress instances appear with their path so far.
:::

## SDK

```rust
let result = client
    .execute("PATHS FROM SupportTicket LIMIT 10")
    .await?;
```
