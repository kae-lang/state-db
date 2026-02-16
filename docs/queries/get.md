# GET

Retrieve a single instance by its ID.

## Syntax

```sql
GET "instance_id"
```

## Response

```json
{
  "success": true,
  "result": {
    "id": "01J5X7K2P3Q4R5S6T7U8V9W0XY",
    "machine": "SupportTicket",
    "state": "in_progress",
    "data": {
      "subject": "Cannot login",
      "priority": "high",
      "assignee": { "id": "agent-1", "role": "support" }
    },
    "created_at": "2026-02-16T10:00:00Z",
    "updated_at": "2026-02-16T12:30:00Z",
    "state_entered_at": "2026-02-16T11:00:00Z",
    "trail_length": 3,
    "version": 3
  }
}
```

## Error

If the instance does not exist, returns HTTP 404:

```json
{
  "success": false,
  "error": "Instance '01J5...' not found"
}
```

## REST Alternative

You can also use the REST endpoint directly:

```
GET /instances/:id
```

## SDK

```rust
let instance = client.get_instance("01J5X7K2P3Q4R5S6T7U8V9W0XY").await?;
println!("State: {}", instance.state);
```

::: tip
Instance IDs are ULIDs (26 characters, time-sortable), not UUIDs.
:::
