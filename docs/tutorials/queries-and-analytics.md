# Tutorial 5: Queries & Analytics

In the previous tutorials, you've been using `GET` and `FIND` for basic queries. SMQL offers much more — aggregations, funnel analysis, path analysis, and audit trail queries. In this tutorial, you'll learn them all.

## Setup

For this tutorial, assume you have a `SupportTicket` machine with many instances in various states. Start the server and load the machine definition:

```bash
smql serve --bind 127.0.0.1:4200
```

```sql
DEFINE MACHINE SupportTicket (
  DATA {
    customer_id : UUID -> REQUIRED
    subject     : TEXT -> REQUIRED
    priority    : ENUM(low, medium, high, critical) -> DEFAULT(medium)
    assignee    : TEXT -> OPTIONAL
    resolution_note : TEXT -> OPTIONAL
  }
  STATES { open, triaged, in_progress, resolved, closed }
  INITIAL STATE open
  TERMINAL STATES { closed }
  TRANSITIONS {
    open -> triaged { GUARD : assignee IS SET }
    triaged -> in_progress {}
    in_progress -> resolved { GUARD : resolution_note IS SET }
    resolved -> closed {}
  }
)
```

Spawn several tickets and transition them to different states so you have data to query.

## FIND — Filtering Instances

### Basic Find

Find all instances of a machine:

```sql
FIND SupportTicket
```

### Filter by State

```sql
FIND SupportTicket WHERE STATE IS open
```

```json
{
  "result": {
    "count": 5,
    "instances": [
      { "id": "01JM...", "state": "open", "data": { "priority": "medium" } },
      { "id": "01JM...", "state": "open", "data": { "priority": "high" } }
    ]
  }
}
```

### Filter by Data Fields

```sql
FIND SupportTicket WHERE priority == "high"
```

### Multiple Conditions

Combine conditions with `AND` and `OR`:

```sql
FIND SupportTicket WHERE STATE IS open AND priority == "critical"
FIND SupportTicket WHERE priority == "high" OR priority == "critical"
```

### Set Membership

```sql
FIND SupportTicket WHERE priority IN ("high", "critical")
```

### Null Checks

```sql
FIND SupportTicket WHERE assignee IS SET
FIND SupportTicket WHERE resolution_note IS NOT SET
```

### Lifecycle Filters

```sql
-- Find instances that are NOT in a terminal state
FIND SupportTicket WHERE ALIVE

-- Find instances in a terminal state
FIND SupportTicket WHERE TERMINATED

-- Find instances that have visited a specific state
FIND SupportTicket WHERE HAS_VISITED in_progress

-- Find instances that never reached a state
FIND SupportTicket WHERE NEVER_VISITED resolved
```

### Sorting and Pagination

```sql
FIND SupportTicket WHERE STATE IS open SORT BY priority DESC
FIND SupportTicket LIMIT 10 OFFSET 20
FIND SupportTicket WHERE ALIVE SORT BY priority ASC LIMIT 5
```

## AGGREGATE — Grouping & Metrics

Aggregations compute metrics across many instances.

### Count All

```sql
AGGREGATE SupportTicket MEASURE COUNT()
```

```json
{
  "result": {
    "rows": [
      { "measures": { "COUNT": 42 } }
    ]
  }
}
```

### Group By State

```sql
AGGREGATE SupportTicket MEASURE COUNT() GROUP BY state
```

```json
{
  "result": {
    "rows": [
      { "group": { "state": "open" }, "measures": { "COUNT": 15 } },
      { "group": { "state": "in_progress" }, "measures": { "COUNT": 12 } },
      { "group": { "state": "resolved" }, "measures": { "COUNT": 8 } },
      { "group": { "state": "closed" }, "measures": { "COUNT": 7 } }
    ]
  }
}
```

### Multiple Measures

Combine count, sum, average, min, max, and percentile:

```sql
AGGREGATE SupportTicket
  MEASURE COUNT()
  MEASURE AVG(satisfaction) AS avg_score
  MEASURE MIN(satisfaction) AS min_score
  MEASURE MAX(satisfaction) AS max_score
  GROUP BY priority
```

```json
{
  "result": {
    "rows": [
      {
        "group": { "priority": "high" },
        "measures": { "COUNT": 10, "avg_score": 3.5, "min_score": 1, "max_score": 5 }
      },
      {
        "group": { "priority": "medium" },
        "measures": { "COUNT": 25, "avg_score": 4.1, "min_score": 2, "max_score": 5 }
      }
    ]
  }
}
```

### Available Aggregate Functions

| Function | Description | Requires Field |
|----------|-------------|----------------|
| `COUNT()` | Number of instances | No |
| `SUM(field)` | Sum of field values | Yes |
| `AVG(field)` | Average of field values | Yes |
| `MIN(field)` | Minimum value | Yes |
| `MAX(field)` | Maximum value | Yes |
| `PERCENTILE(field, p)` | Percentile (0-100) | Yes |

### Percentile

```sql
AGGREGATE SupportTicket
  MEASURE PERCENTILE(satisfaction, 50) AS p50
  MEASURE PERCENTILE(satisfaction, 95) AS p95
  MEASURE PERCENTILE(satisfaction, 99) AS p99
```

## TRAIL — Audit History

Every spawn and transition is recorded in an immutable trail.

### View an Instance's Trail

```sql
TRAIL OF SupportTicket "01JM..."
```

```json
{
  "result": {
    "count": 4,
    "entries": [
      { "sequence": 0, "from_state": "", "to_state": "open", "actor": null, "memo": null },
      { "sequence": 1, "from_state": "open", "to_state": "triaged", "actor": "agent_1", "memo": null },
      { "sequence": 2, "from_state": "triaged", "to_state": "in_progress", "actor": "agent_1", "memo": "Starting investigation" },
      { "sequence": 3, "from_state": "in_progress", "to_state": "resolved", "actor": "agent_1", "memo": "Fixed" }
    ]
  }
}
```

Key properties:
- **Sequence 0** is always the spawn event (empty `from_state`)
- **actor** is the `AS` clause value (or null for system/anonymous)
- **memo** is the `MEMO` clause value (free-form audit note)

### Adding Memos

Use the `MEMO` clause to attach audit notes to transitions:

```sql
TRANSITION "01JM..." TO resolved
  WITH { resolution_note: "Cache cleared" }
  AS "agent_1"
  MEMO "Customer confirmed fix works"
```

The memo appears in the trail entry and is useful for compliance and audit requirements.

## PATHS — State Sequence Analysis

PATHS shows you the actual routes instances take through your state machine:

```sql
PATHS FROM SupportTicket
```

```json
{
  "result": {
    "paths": [
      { "path": ["", "open", "triaged", "in_progress", "resolved", "closed"], "count": 15 },
      { "path": ["", "open", "triaged", "in_progress", "resolved"], "count": 8 },
      { "path": ["", "open", "triaged", "in_progress"], "count": 12 },
      { "path": ["", "open", "triaged"], "count": 3 },
      { "path": ["", "open"], "count": 5 }
    ]
  }
}
```

This tells you:
- 15 tickets completed the full lifecycle to `closed`
- 8 tickets are sitting in `resolved` (maybe waiting for the auto-close timeout)
- 5 tickets are still in `open` (never triaged)

PATHS helps you identify bottlenecks and unusual flows in your process.

## FUNNEL — Conversion Analysis

FUNNEL measures how many instances progress through a sequence of states:

```sql
FUNNEL SupportTicket THROUGH open, triaged, in_progress, resolved, closed
```

```json
{
  "result": {
    "stages": [
      { "state": "open", "count": 43, "conversion_rate": 1.0 },
      { "state": "triaged", "count": 38, "conversion_rate": 0.884 },
      { "state": "in_progress", "count": 35, "conversion_rate": 0.921 },
      { "state": "resolved", "count": 23, "conversion_rate": 0.657 },
      { "state": "closed", "count": 15, "conversion_rate": 0.652 }
    ]
  }
}
```

Reading the funnel:
- **43** tickets were spawned (entered `open`)
- **38** (88.4%) were triaged — 5 tickets are stuck in `open`
- **35** (92.1% of triaged) moved to `in_progress`
- **23** (65.7% of in_progress) were resolved — the biggest drop-off
- **15** (65.2% of resolved) were closed

The `conversion_rate` is relative to the **previous** stage, making it easy to spot where the biggest bottlenecks are.

::: tip
Use FUNNEL to identify process bottlenecks. In this example, the `in_progress -> resolved` step has a 65.7% conversion rate — you might want to investigate why a third of tickets stall there.
:::

## COMPARE PATHS — Segment Analysis

Compare how different segments move through the state machine:

```sql
COMPARE PATHS SupportTicket SEGMENT BY priority
```

This shows the path distribution broken down by priority level, helping you understand if high-priority tickets take different routes than low-priority ones.

## Combining Queries

In practice, you'll use multiple query types together to build a complete picture:

```sql
-- How many tickets are stuck?
AGGREGATE SupportTicket MEASURE COUNT() GROUP BY state

-- Which tickets are stuck in open?
FIND SupportTicket WHERE STATE IS open SORT BY priority DESC

-- What's the typical lifecycle?
FUNNEL SupportTicket THROUGH open, triaged, in_progress, resolved, closed

-- What paths do high-priority tickets take?
PATHS FROM SupportTicket WHERE priority == "critical"
```

## What You Learned

| Concept | Summary |
|---------|---------|
| `FIND` | Filter instances by state, data fields, lifecycle status |
| `SORT BY` / `LIMIT` / `OFFSET` | Ordering and pagination |
| `ALIVE` / `TERMINATED` | Lifecycle status predicates |
| `HAS_VISITED` / `NEVER_VISITED` | Historical state predicates |
| `AGGREGATE` | Compute COUNT, SUM, AVG, MIN, MAX, PERCENTILE |
| `GROUP BY` | Break aggregations down by state or field |
| `TRAIL` | Immutable audit log of every state change |
| `MEMO` | Free-form audit notes attached to transitions |
| `PATHS` | Distribution of state sequences across instances |
| `FUNNEL` | Conversion rates through a sequence of states |
| `COMPARE PATHS` | Path analysis segmented by a data field |

## Next Step

You've mastered SMQL's query capabilities. In the [final tutorial](./production-deployment), you'll learn how to deploy SMQL for production use with persistent storage, monitoring, the Rust SDK, and schema evolution.
