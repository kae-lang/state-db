# TRAIL

Retrieve the complete transition history of an instance.

## Syntax

```sql
TRAIL OF "instance_id"
```

## Response

```json
{
  "success": true,
  "result": {
    "count": 4,
    "entries": [
      {
        "sequence": 0,
        "from_state": "",
        "to_state": "open",
        "actor": null,
        "memo": null,
        "timestamp": "2026-02-16T10:00:00Z"
      },
      {
        "sequence": 1,
        "from_state": "open",
        "to_state": "triaged",
        "actor": "agent-1",
        "memo": null,
        "timestamp": "2026-02-16T10:15:00Z"
      },
      {
        "sequence": 2,
        "from_state": "triaged",
        "to_state": "in_progress",
        "actor": "agent-1",
        "memo": "Starting investigation",
        "timestamp": "2026-02-16T10:30:00Z"
      },
      {
        "sequence": 3,
        "from_state": "in_progress",
        "to_state": "resolved",
        "actor": "agent-1",
        "memo": "Root cause identified and fixed",
        "timestamp": "2026-02-16T14:00:00Z"
      }
    ]
  }
}
```

::: info
Sequence 0 is always the spawn event, with an empty `from_state`.
:::

## SDK

```rust
let trail = client.trail("01J5X7K2P3Q4R5S6T7U8V9W0XY").await?;
for entry in &trail {
    println!("{}: {} -> {} (by {:?})",
        entry.sequence, entry.from_state, entry.to_state, entry.actor);
}
```
