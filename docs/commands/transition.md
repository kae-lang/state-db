# TRANSITION

The `TRANSITION` command moves an instance from its current state to a new state.

## Syntax

```sql
TRANSITION MachineName "instance_id" TO target_state
TRANSITION MachineName "instance_id" TO target_state AS { id: "user-1", role: "admin" }
TRANSITION MachineName "instance_id" TO target_state WITH { field: value }
TRANSITION MachineName "instance_id" TO target_state MEMO "Reason for transition"
```

## Clauses

| Clause | Purpose | Example |
|--------|---------|---------|
| `TO` | Target state (required) | `TO resolved` |
| `AS` | Actor identity | `AS { id: "u1", role: "admin" }` |
| `WITH` | Data to merge | `WITH { note: "Fixed" }` |
| `MEMO` | Human-readable note | `MEMO "Customer confirmed"` |

## Response

```json
{
  "success": true,
  "result": {
    "from_state": "open",
    "to_state": "triaged",
    "instance": {
      "id": "01J5...",
      "machine": "SupportTicket",
      "state": "triaged",
      "data": { ... },
      "version": 2,
      ...
    }
  }
}
```

## Guards

If any guard fails, the transition is denied with HTTP 409 Conflict:

```json
{
  "success": false,
  "error": "Transition denied: guard failed for open -> triaged"
}
```

## THROUGH

Multi-hop transition through intermediate states:

```sql
TRANSITION SupportTicket "01J5..." TO resolved THROUGH in_progress
```

This transitions `open -> in_progress -> resolved` in sequence. Each hop evaluates its own guards.

## OR STAY

If the transition fails, keep the instance in its current state instead of returning an error:

```sql
TRANSITION SupportTicket "01J5..." TO resolved OR STAY
```

## CASCADE

Cascade the transition to child instances:

```sql
TRANSITION SupportTicket "01J5..." TO cancelled CASCADE
```

This recursively transitions all children to their first terminal state.

::: warning
CASCADE only attempts the first terminal state. If its guard fails, the child remains in its current state.
:::

## Examples

::: code-group
```bash [curl]
curl -X POST http://localhost:4200/execute \
  -H 'Content-Type: application/json' \
  -d '{
    "smql": "TRANSITION SupportTicket \"01J5X7K2P3Q4R5S6T7U8V9W0XY\" TO triaged AS { id: \"agent-1\", role: \"support\" } WITH { assignee: { id: \"agent-1\", role: \"support\" } }"
  }'
```

```rust [SDK]
use smql_sdk::types::TransitionOptions;

let result = client.transition(
    "01J5X7K2P3Q4R5S6T7U8V9W0XY",
    "triaged",
    TransitionOptions {
        as_actor: Some(r#"{ id: "agent-1", role: "support" }"#.into()),
        with_data: vec![("assignee".into(), serde_json::json!({"id": "agent-1", "role": "support"}))],
        memo: Some("Assigned to agent".into()),
    },
).await?;
```
:::
