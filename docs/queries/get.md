# GET

Retrieve a single instance by its machine name and ID.

## Syntax

```smql
GET MachineName "instance_id"
```

The instance ID must be a quoted string (ULIDs start with digits, so unquoted form is not valid):

```smql
GET SupportTicket "01J5X7K2P3Q4R5S6T7U8V9W0XY"
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
    "created_at": "2026-02-16T10:00:00+00:00",
    "updated_at": "2026-02-16T12:30:00+00:00",
    "state_entered_at": "2026-02-16T11:00:00+00:00",
    "trail_length": 3,
    "version": 3
  }
}
```

## Errors

If the instance does not exist, returns HTTP `404 Not Found`:

```json
{
  "success": false,
  "error": "Instance '01J5...' not found"
}
```

If the instance exists but belongs to a different machine, it also returns `404`.

## REST Alternative

You can also use the REST endpoint directly (no machine name needed):

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
