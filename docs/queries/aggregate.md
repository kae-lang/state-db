# Aggregate Queries

Compute aggregate measures across instances.

## Syntax

```sql
COUNT MachineName
COUNT MachineName WHERE <predicate>
COUNT MachineName GROUP BY state
COUNT MachineName GROUP BY field_name

SUM(field) FROM MachineName WHERE <predicate>
AVG(field) FROM MachineName
MIN(field) FROM MachineName
MAX(field) FROM MachineName
```

## Examples

### Count by state

```sql
COUNT SupportTicket GROUP BY state
```

Response:
```json
{
  "success": true,
  "result": {
    "rows": [
      { "group": { "state": "open" }, "measures": { "count": 15 } },
      { "group": { "state": "in_progress" }, "measures": { "count": 8 } },
      { "group": { "state": "resolved" }, "measures": { "count": 42 } }
    ]
  }
}
```

### Count by data field

```sql
COUNT SupportTicket GROUP BY priority
```

### Sum

```sql
SUM(total) FROM Order WHERE STATE IS paid
```

### Average

```sql
AVG(satisfaction) FROM SupportTicket WHERE STATE IS closed
```

## SDK

```rust
let result = client.aggregate("SupportTicket")
    .measure("COUNT()")
    .group_by_state()
    .execute()
    .await?;
```
