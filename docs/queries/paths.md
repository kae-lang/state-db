# PATHS

Analyze the transition paths taken by instances of a machine.

## Syntax

```sql
PATHS MachineName
```

## Response

```json
{
  "success": true,
  "result": {
    "paths": [
      { "path": ["open", "triaged", "in_progress", "resolved", "closed"], "count": 35 },
      { "path": ["open", "triaged", "in_progress", "waiting_on_customer", "in_progress", "resolved", "closed"], "count": 12 },
      { "path": ["open", "triaged", "in_progress", "resolved", "reopened", "in_progress", "resolved", "closed"], "count": 5 }
    ]
  }
}
```

Each path is a sequence of states, and `count` is how many instances followed that exact path.

::: tip
PATHS analysis works on completed (terminal state) instances. Instances still in progress are not included.
:::
