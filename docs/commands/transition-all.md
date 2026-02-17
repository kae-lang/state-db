# TRANSITION ALL

`TRANSITION ALL` transitions all matching instances of a machine at once.

## Syntax

```smql
TRANSITION ALL MachineName WHERE <predicate> TO target_state
```

## Examples

```smql
-- Close all resolved tickets
TRANSITION ALL SupportTicket WHERE STATE IS resolved TO closed

-- Cancel all draft orders older than 30 days
TRANSITION ALL Order WHERE STATE IS draft AND elapsed() > 30d TO cancelled
```

This finds all matching instances and transitions each one individually to the target state.

## Guards

Each instance is evaluated individually against the transition's guards. Instances that fail the guard are skipped and reported in the `failures` array.

## Response

```json
{
  "success": true,
  "result": {
    "action": "batch_transition",
    "machine": "SupportTicket",
    "matched": 15,
    "transitioned": 12,
    "failed": 3,
    "failures": [
      {
        "instance_id": "01HXYZ...",
        "error": "Transition denied: guard condition not met for resolved -> closed"
      }
    ]
  }
}
```

| Field | Description |
|-------|-------------|
| `matched` | Total instances matching the WHERE filter |
| `transitioned` | Instances that were successfully transitioned |
| `failed` | Count of instances that failed (guard rejection, etc.) |
| `failures` | Array of failure details with instance ID and error message |

When no instances match the WHERE filter, `matched`, `transitioned`, and `failed` are all `0` and `failures` is an empty array.

::: info
`TRANSITION ALL` is useful for batch operations like closing all resolved tickets or cancelling all pending orders. Each transition fires hooks and records a trail entry just like a single `TRANSITION`.
:::
