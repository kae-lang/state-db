# FUNNEL

Analyze conversion through a sequence of states.

## Syntax

```sql
FUNNEL MachineName THROUGH state1, state2, state3
```

## Example

```sql
FUNNEL SupportTicket THROUGH open, triaged, in_progress, resolved, closed
```

## Response

```json
{
  "success": true,
  "result": {
    "stages": [
      { "state": "open", "count": 100, "conversion_rate": 1.0 },
      { "state": "triaged", "count": 85, "conversion_rate": 0.85 },
      { "state": "in_progress", "count": 80, "conversion_rate": 0.94 },
      { "state": "resolved", "count": 72, "conversion_rate": 0.90 },
      { "state": "closed", "count": 70, "conversion_rate": 0.97 }
    ]
  }
}
```

The `conversion_rate` is the ratio of instances that reached this state compared to the previous state.
