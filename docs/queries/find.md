# FIND

Query instances of a machine with optional filters, sorting, and pagination.

## Syntax

```smql
FIND MachineName
  [WHERE <predicate>]
  [SORT BY field ASC|DESC [, field ASC|DESC ...]]
  [LIMIT n]
  [OFFSET n]
  [AFTER "cursor_ulid"]
```

Clauses must appear in this order: `WHERE` → `SORT BY` → `LIMIT` → `OFFSET` → `AFTER`.

## Predicates

See [Filter Predicates](./filter-predicates) for the full list.

```smql
FIND SupportTicket WHERE STATE IS open
FIND SupportTicket WHERE priority == "critical" AND assignee IS SET
FIND SupportTicket WHERE STATE IN {open, triaged, in_progress}
```

## Sorting

```smql
FIND SupportTicket WHERE STATE IS open SORT BY created_at DESC
FIND SupportTicket SORT BY priority ASC, created_at DESC
```

The `BY` keyword is optional: `SORT priority DESC` also works.

## Offset Pagination

```smql
FIND SupportTicket SORT BY created_at DESC LIMIT 20 OFFSET 40
```

## Cursor-Based Pagination

For large datasets, cursor-based pagination is more efficient than OFFSET. Use the `AFTER` clause with the ULID of the last instance from the previous page:

```smql
-- First page
FIND SupportTicket WHERE STATE IS open LIMIT 20

-- Next page (use next_cursor from the previous response)
FIND SupportTicket WHERE STATE IS open LIMIT 20 AFTER "01HWZK4G5C8T3RNMK1VNSH7HYM"
```

The `AFTER` clause uses ULID keyset pagination — instances are returned in ULID order, starting after the given ID. This avoids the performance degradation of deep OFFSET pages.

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
        "data": { "subject": "Login broken", "priority": "high" },
        "created_at": "2026-02-16T10:00:00+00:00",
        "updated_at": "2026-02-16T10:00:00+00:00",
        "state_entered_at": "2026-02-16T10:00:00+00:00",
        "trail_length": 1,
        "version": 1
      },
      {
        "id": "01J6...",
        "machine": "SupportTicket",
        "state": "open",
        "data": { "subject": "Signup error", "priority": "medium" },
        "created_at": "2026-02-16T11:00:00+00:00",
        "updated_at": "2026-02-16T11:00:00+00:00",
        "state_entered_at": "2026-02-16T11:00:00+00:00",
        "trail_length": 1,
        "version": 1
      }
    ],
    "next_cursor": "01J6..."
  }
}
```

The `next_cursor` field contains the ULID of the last instance in the result set. Pass it to `AFTER` in the next query to get the next page. When there are no results, `next_cursor` is absent.

An empty result set returns:

```json
{
  "success": true,
  "result": {
    "count": 0,
    "instances": []
  }
}
```

::: warning
When combining `WHERE` with `LIMIT`, the limit is applied at the storage level before the WHERE filter. This means you may receive fewer results than the limit if the storage-level results are further filtered by the WHERE clause. For deterministic pagination, prefer cursor-based pagination with `AFTER`.
:::

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
