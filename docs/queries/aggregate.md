# AGGREGATE

Compute aggregate measures across instances of a machine, optionally grouped by state or data field.

## Syntax

```smql
AGGREGATE MachineName
  MEASURE func[(field)] [AS alias] [, func[(field)] [AS alias] ...]
  [WHERE <predicate>]
  [GROUP BY STATE | field_name [, ...]]
```

The `MEASURE`, `WHERE`, and `GROUP BY` clauses can appear in any order.

## Aggregate Functions

| Function | Description | Requires Field |
|----------|-------------|----------------|
| `COUNT()` | Number of instances | No |
| `SUM(field)` | Sum of a numeric field | Yes |
| `AVG(field)` | Average of a numeric field | Yes |
| `MIN(field)` | Minimum value of a field | Yes |
| `MAX(field)` | Maximum value of a field | Yes |
| `PERCENTILE(field)` | Percentile of a numeric field | Yes |

## Examples

### Count all instances

```smql
AGGREGATE SupportTicket MEASURE COUNT()
```

### Count by state

```smql
AGGREGATE SupportTicket MEASURE COUNT() GROUP BY STATE
```

### Multiple measures

```smql
AGGREGATE SupportTicket
  MEASURE COUNT() AS total, SUM(points) AS total_points, AVG(points) AS avg_points
```

### Group by data field

```smql
AGGREGATE SupportTicket MEASURE COUNT(), SUM(points) GROUP BY priority
```

### With a WHERE filter

```smql
AGGREGATE SupportTicket
  MEASURE COUNT() AS total, AVG(satisfaction) AS avg_sat
  WHERE STATE IS closed
  GROUP BY priority
```

### Min and Max

```smql
AGGREGATE Order MEASURE MIN(total), MAX(total)
```

## Response

Each result row contains a `group` key (the GROUP BY values, empty when no GROUP BY is used) and a `measures` key (the computed values). Measure keys use the alias if provided, otherwise the function name in uppercase.

**Ungrouped:**

```json
{
  "success": true,
  "result": {
    "rows": [
      {
        "group": {},
        "measures": { "COUNT": 64 }
      }
    ]
  }
}
```

**Grouped by state:**

```json
{
  "success": true,
  "result": {
    "rows": [
      {
        "group": { "state": "open" },
        "measures": { "COUNT": 15 }
      },
      {
        "group": { "state": "in_progress" },
        "measures": { "COUNT": 8 }
      },
      {
        "group": { "state": "resolved" },
        "measures": { "COUNT": 42 }
      }
    ]
  }
}
```

**Multiple measures with aliases:**

```json
{
  "success": true,
  "result": {
    "rows": [
      {
        "group": { "priority": "high" },
        "measures": { "total": 12, "avg_points": 6.5 }
      },
      {
        "group": { "priority": "low" },
        "measures": { "total": 30, "avg_points": 2.1 }
      }
    ]
  }
}
```

::: tip
When no instances match the filter (or the machine has no instances), AGGREGATE still returns one row with `COUNT` = 0 and other measures as `null`.
:::

## SDK

```rust
let result = client.aggregate("SupportTicket")
    .measure("COUNT()")
    .group_by_state()
    .execute()
    .await?;

let result = client.aggregate("SupportTicket")
    .measure("SUM(points)")
    .measure("AVG(points)")
    .group_by_field("priority")
    .execute()
    .await?;
```
