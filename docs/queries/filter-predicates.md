# Filter Predicates

Predicates are used in `WHERE` clauses for `FIND`, `COUNT`, and `TRANSITION ALL` queries.

## State Predicates

| Predicate | Description | Example |
|-----------|-------------|---------|
| `STATE IS state` | Instance is in exact state | `STATE IS open` |
| `STATE IN (s1, s2)` | Instance is in one of listed states | `STATE IN (open, triaged)` |
| `ALIVE` | Instance is not in a terminal state | `ALIVE` |

## Data Predicates

| Predicate | Description | Example |
|-----------|-------------|---------|
| `field == value` | Equality | `priority == "critical"` |
| `field != value` | Inequality | `status != "draft"` |
| `field > value` | Greater than | `count > 10` |
| `field < value` | Less than | `age < 30` |
| `field >= value` | Greater or equal | `version >= 2` |
| `field <= value` | Less or equal | `score <= 100` |
| `field IS SET` | Field is not null | `assignee IS SET` |
| `field IS NOT SET` | Field is null | `assignee IS NOT SET` |
| `field IN (a, b, c)` | Value in list | `priority IN ("high", "critical")` |

## Time Predicates

| Predicate | Description | Example |
|-----------|-------------|---------|
| `HAS_VISITED state` | Instance has been in state at some point | `HAS_VISITED in_progress` |

## Combining Predicates

Use `AND` and `OR` with parentheses:

```sql
FIND SupportTicket WHERE STATE IS open AND priority == "critical"
FIND SupportTicket WHERE (STATE IS open OR STATE IS triaged) AND assignee IS SET
```

::: info
`STUCK_IN` is a reserved keyword but not yet available as a query filter.
:::
