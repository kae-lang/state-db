# Built-in Functions Reference

SMQL provides built-in functions for temporal calculations, aggregation, and state inspection. This page documents every available function.

## Temporal Functions

These functions are available in guards, WHERE clauses, and expressions.

### elapsed()

Returns the time elapsed since the instance was spawned.

**Returns:** `Duration`

```sql
-- Find instances spawned more than 24 hours ago
FIND Machine WHERE elapsed() > 24h

-- Guard: only allow after 1 hour
GUARD : elapsed() > 1h
```

### elapsed_in_state()

Returns the time the instance has been in its current state.

**Returns:** `Duration`

```sql
-- Find instances stuck in a state for over 2 hours
FIND Machine WHERE elapsed_in_state() > 2h

-- Guard: must be in state for at least 5 minutes
GUARD : elapsed_in_state() > 5m
```

### elapsed_since(state)

Returns the time since the instance was last in the specified state. If the instance has never been in that state, returns `Null`.

**Parameters:**
- `state` — the state name (unquoted identifier)

**Returns:** `Duration` or `Null`

```sql
-- Only allow reopen within 30 days of resolution
GUARD : elapsed_since(resolved) < 30d

-- Auto-close after 7 days
GUARD : elapsed_since(resolved) >= 7d
```

### timeout_remaining()

Returns the time remaining on the active timeout for the current state, or `Null` if no timeout is active.

**Returns:** `Duration` or `Null`

```sql
-- Find instances with less than 1 hour before timeout fires
FIND Machine WHERE timeout_remaining() < 1h

-- Guard: only allow transition if timeout is close
GUARD : timeout_remaining() < 30m
```

### NOW()

Returns the current date and time in UTC.

**Returns:** `DateTime`

```sql
-- Available as a value in expressions
MUTATE : last_checked = NOW()
```

### TODAY()

Returns the current date in UTC (without time component).

**Returns:** `Date`

```sql
MUTATE : review_date = TODAY()
```

## Aggregation Functions

These functions are used in `AGGREGATE` queries with the `MEASURE` clause.

### COUNT()

Counts the number of instances matching the query criteria.

**Parameters:** None

```sql
AGGREGATE SupportTicket MEASURE COUNT()
AGGREGATE SupportTicket MEASURE COUNT() GROUP BY state
```

**Output key:** `COUNT` (or custom alias)

### SUM(field)

Computes the sum of a numeric field across matching instances.

**Parameters:**
- `field` — the data field name

**Supported types:** `INT`, `FLOAT`

```sql
AGGREGATE Order MEASURE SUM(total)
AGGREGATE Order MEASURE SUM(total) AS revenue GROUP BY state
```

### AVG(field)

Computes the arithmetic mean of a numeric field across matching instances.

**Parameters:**
- `field` — the data field name

**Supported types:** `INT`, `FLOAT`

```sql
AGGREGATE SupportTicket MEASURE AVG(satisfaction)
AGGREGATE SupportTicket MEASURE AVG(satisfaction) GROUP BY priority
```

### MIN(field)

Returns the minimum value of a numeric field across matching instances.

**Parameters:**
- `field` — the data field name

**Supported types:** `INT`, `FLOAT`

```sql
AGGREGATE SupportTicket MEASURE MIN(satisfaction)
```

### MAX(field)

Returns the maximum value of a numeric field across matching instances.

**Parameters:**
- `field` — the data field name

**Supported types:** `INT`, `FLOAT`

```sql
AGGREGATE SupportTicket MEASURE MAX(satisfaction) AS highest_score
```

### PERCENTILE(field, p)

Returns the p-th percentile of a numeric field. The percentile value `p` is between 0 and 100.

**Parameters:**
- `field` — the data field name
- `p` — the percentile value (0-100)

**Supported types:** `INT`, `FLOAT`

```sql
AGGREGATE SupportTicket
  MEASURE PERCENTILE(satisfaction, 50) AS p50
  MEASURE PERCENTILE(satisfaction, 95) AS p95
  MEASURE PERCENTILE(satisfaction, 99) AS p99
```

::: tip
Common percentiles: p50 (median), p90, p95, p99. Use these to understand the distribution of values, not just the average.
:::

## Collection Functions

These functions operate on child collections in composition.

### ALL(collection, predicate)

Returns `true` if every item in the collection matches the predicate.

**Parameters:**
- `collection` — a child relationship name
- `predicate` — a boolean expression, typically `STATE IS state`

**Empty collection:** Returns `true` (vacuous truth)

```sql
GUARD : ALL(items, STATE IS confirmed)
GUARD : ALL(stages, STATE IS passed)
```

### ANY(collection, predicate)

Returns `true` if at least one item in the collection matches the predicate.

**Parameters:**
- `collection` — a child relationship name
- `predicate` — a boolean expression

**Empty collection:** Returns `false`

```sql
GUARD : ANY(stages, STATE IS failed)
GUARD : ANY(items, STATE IS backordered)
```

### COUNT (field access)

Access the count of a child collection:

```sql
GUARD : items.count > 0
```

## Function Summary

| Function | Context | Returns | Description |
|----------|---------|---------|-------------|
| `elapsed()` | Guard, WHERE | Duration | Time since spawn |
| `elapsed_in_state()` | Guard, WHERE | Duration | Time in current state |
| `elapsed_since(state)` | Guard, WHERE | Duration/Null | Time since last in state |
| `timeout_remaining()` | Guard, WHERE | Duration/Null | Time until timeout fires |
| `NOW()` | Expression | DateTime | Current UTC datetime |
| `TODAY()` | Expression | Date | Current UTC date |
| `COUNT()` | AGGREGATE | Int | Count of instances |
| `SUM(field)` | AGGREGATE | Numeric | Sum of field values |
| `AVG(field)` | AGGREGATE | Float | Mean of field values |
| `MIN(field)` | AGGREGATE | Numeric | Minimum field value |
| `MAX(field)` | AGGREGATE | Numeric | Maximum field value |
| `PERCENTILE(field, p)` | AGGREGATE | Float | p-th percentile |
| `ALL(coll, pred)` | Guard | Bool | All children match |
| `ANY(coll, pred)` | Guard | Bool | Any child matches |
