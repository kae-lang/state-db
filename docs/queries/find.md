# FIND

Query instances of a machine with optional filters, sorting, and pagination.

## Syntax

```sql
FIND MachineName
FIND MachineName WHERE <predicate>
FIND MachineName WHERE <predicate> SORT BY field ASC LIMIT 10 OFFSET 0
```

## Predicates

See [Filter Predicates](./filter-predicates) for the full list.

```sql
FIND SupportTicket WHERE STATE IS open
FIND SupportTicket WHERE priority == "critical" AND assignee IS SET
FIND SupportTicket WHERE STATE IN (open, triaged, in_progress)
```

## Sorting

```sql
FIND SupportTicket WHERE STATE IS open SORT BY created_at DESC
FIND SupportTicket SORT BY priority ASC, created_at DESC
```

## Pagination

```sql
FIND SupportTicket SORT BY created_at DESC LIMIT 20 OFFSET 40
```

## Response

```json
{
  "success": true,
  "result": {
    "count": 2,
    "instances": [
      {
        "id": "01J5...",
        "machine": "SupportTicket",
        "state": "open",
        "data": { ... },
        ...
      },
      {
        "id": "01J6...",
        "machine": "SupportTicket",
        "state": "open",
        "data": { ... },
        ...
      }
    ]
  }
}
```

## SDK

```rust
let results = client.find("SupportTicket")
    .in_state("open")
    .sort_by("created_at", "DESC")
    .limit(20)
    .execute()
    .await?;

// Or just the count
let count = client.find("SupportTicket")
    .in_state("open")
    .count()
    .await?;
```
