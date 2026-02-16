# SPAWN

The `SPAWN` command creates a new instance of a machine.

## Syntax

```sql
SPAWN MachineName { field: value, field2: value2 }
```

## Parameters

| Parameter | Description |
|-----------|-------------|
| Machine name | The registered machine type |
| Data block | Initial field values in `{ key: value }` format |

::: warning
The data block uses colons, not equals: `{ title: "Hello" }` not `{ title = "Hello" }`.
Even with no data, braces are required: `SPAWN Machine {}`.
:::

## Response

```json
{
  "success": true,
  "result": {
    "id": "01J5X7K2P3Q4R5S6T7U8V9W0XY",
    "machine": "Task",
    "state": "todo",
    "data": { "title": "Hello" },
    "created_at": "2026-02-16T10:00:00Z",
    "updated_at": "2026-02-16T10:00:00Z",
    "state_entered_at": "2026-02-16T10:00:00Z",
    "trail_length": 1,
    "version": 1
  }
}
```

Instance IDs are ULIDs (26-character, time-sortable identifiers).

## SPAWN BATCH

Spawn multiple instances at once:

```sql
SPAWN BATCH Task [
  { title: "Task 1" },
  { title: "Task 2" },
  { title: "Task 3" }
]
```

## THEN TRANSITION

Spawn and immediately transition:

```sql
SPAWN Task { title: "Urgent" } THEN TRANSITION TO doing
```

## Validation

At spawn time, the engine validates:
- All REQUIRED fields are present
- Field types match the schema
- Constraints (MIN, MAX, RANGE, PATTERN, UNIQUE) are satisfied
- DEFAULT values are applied for missing optional fields

## Examples

::: code-group
```bash [curl]
curl -X POST http://localhost:4200/execute \
  -H 'Content-Type: application/json' \
  -d '{
    "smql": "SPAWN SupportTicket { customer_id: \"550e8400-e29b-41d4-a716-446655440000\", subject: \"Cannot login\", description: \"Getting 401 error\" }"
  }'
```

```bash [REPL]
> SPAWN SupportTicket {
    customer_id: "550e8400-e29b-41d4-a716-446655440000",
    subject: "Cannot login",
    description: "Getting 401 error"
  }
```

```rust [SDK]
let instance = client.spawn("SupportTicket", serde_json::json!({
    "customer_id": "550e8400-e29b-41d4-a716-446655440000",
    "subject": "Cannot login",
    "description": "Getting 401 error"
})).await?;
```
:::

::: tip
The trail starts at sequence 0 with a spawn event (empty `from_state`).
:::
