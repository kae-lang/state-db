# Queries

The SDK provides two builder types for querying instances: `FindBuilder` for retrieving instances and `AggregateBuilder` for computing aggregations. Both are created from a `SmqlClient` and use a chainable API.

## FindBuilder

`FindBuilder` constructs a `FIND` query and sends it to the server. Start a find query with `client.find(machine)`.

### Basic Usage

```rust
let instances = client.find("Order")
    .execute()
    .await?;

for inst in &instances {
    println!("{} — state: {}, data: {}", inst.id, inst.state, inst.data);
}
```

### Filtering by State

Use `in_state` to restrict results to instances currently in a specific state.

```rust
let pending = client.find("Order")
    .in_state("pending")
    .execute()
    .await?;
```

This generates:

```sql
FIND Order IN STATE pending
```

### Stuck-In Filter

Use `stuck_in` to find instances that have been in a state for longer than a given duration.

```rust
let stale = client.find("SupportTicket")
    .stuck_in("open", "2d")
    .execute()
    .await?;
```

This generates:

```sql
FIND SupportTicket STUCK_IN open FOR 2d
```

Duration strings follow SMQL syntax: `30s`, `5m`, `2h`, `1d`, `1w`.

### WHERE Clause

Use `where_clause` to filter on instance data fields.

```rust
let high_value = client.find("Order")
    .in_state("confirmed")
    .where_clause("quantity > 10")
    .execute()
    .await?;
```

This generates:

```sql
FIND Order IN STATE confirmed WHERE quantity > 10
```

The expression string uses SMQL expression syntax. Supported operators include `==`, `!=`, `>`, `<`, `>=`, `<=`, `&&`, `||`.

### Sorting

Use `sort_by` to order results by a field. The second argument is the sort direction: `"ASC"` or `"DESC"`.

```rust
let newest_first = client.find("Order")
    .sort_by("created_at", "DESC")
    .execute()
    .await?;
```

This generates:

```sql
FIND Order SORT BY created_at DESC
```

### Pagination

Use `limit` and `offset` for pagination.

```rust
let page_size = 25;
let page = 2;

let results = client.find("Order")
    .in_state("shipped")
    .sort_by("updated_at", "DESC")
    .limit(page_size)
    .offset((page - 1) * page_size)
    .execute()
    .await?;
```

This generates:

```sql
FIND Order IN STATE shipped SORT BY updated_at DESC LIMIT 25 OFFSET 25
```

### Counting

Use `count` instead of `execute` to get the number of matching instances without fetching them.

```rust
let total = client.find("Order")
    .in_state("pending")
    .count()
    .await?;

println!("Pending orders: {}", total);
```

This generates:

```sql
FIND Order IN STATE pending COUNT
```

### Full Chaining Example

All filter methods can be combined in a single chain:

```rust
let results = client.find("SupportTicket")
    .in_state("open")
    .where_clause("priority == 1")
    .sort_by("created_at", "ASC")
    .limit(10)
    .offset(0)
    .execute()
    .await?;
```

## AggregateBuilder

`AggregateBuilder` constructs an `AGGREGATE` query for computing summaries across instances. Start with `client.aggregate(machine)`.

### Measure

Use `measure` to specify the aggregation function. SMQL supports `COUNT`, `SUM(field)`, `AVG(field)`, `MIN(field)`, and `MAX(field)`.

```rust
let total = client.aggregate("Order")
    .measure("COUNT")
    .execute()
    .await?;

println!("Total orders: {}", total);
```

```rust
let avg_amount = client.aggregate("Invoice")
    .measure("AVG(amount)")
    .execute()
    .await?;
```

### Group by State

Use `group_by_state` to get per-state breakdowns.

```rust
let breakdown = client.aggregate("Order")
    .measure("COUNT")
    .group_by_state()
    .execute()
    .await?;

println!("{}", serde_json::to_string_pretty(&breakdown)?);
// Example output:
// {
//   "pending": 12,
//   "confirmed": 8,
//   "shipped": 3,
//   "delivered": 42
// }
```

This generates:

```sql
AGGREGATE Order MEASURE COUNT GROUP BY STATE
```

### Group by Field

Use `group_by_field` to group by a data field.

```rust
let by_priority = client.aggregate("SupportTicket")
    .measure("COUNT")
    .group_by_field("priority")
    .execute()
    .await?;
```

This generates:

```sql
AGGREGATE SupportTicket MEASURE COUNT GROUP BY priority
```

### Combining Measure and Group By

```rust
let revenue_by_region = client.aggregate("Order")
    .measure("SUM(total)")
    .group_by_field("region")
    .execute()
    .await?;
```

## Return Types

### FindBuilder

| Method | Return Type | Description |
|--------|-------------|-------------|
| `execute()` | `SdkResult<Vec<InstanceResponse>>` | Matching instances |
| `count()` | `SdkResult<u64>` | Count of matching instances |

### AggregateBuilder

| Method | Return Type | Description |
|--------|-------------|-------------|
| `execute()` | `SdkResult<Value>` | JSON value with aggregation results |

## Builder Method Reference

### FindBuilder

| Method | Parameter | Description |
|--------|-----------|-------------|
| `in_state(state)` | `&str` | Filter to instances in the given state |
| `stuck_in(state, duration)` | `&str, &str` | Filter to instances stuck in a state beyond the duration |
| `where_clause(expr)` | `&str` | Filter by a data expression |
| `sort_by(field, direction)` | `&str, &str` | Sort results (`"ASC"` or `"DESC"`) |
| `limit(n)` | `u64` | Maximum number of results |
| `offset(n)` | `u64` | Number of results to skip |

### AggregateBuilder

| Method | Parameter | Description |
|--------|-----------|-------------|
| `measure(m)` | `&str` | Aggregation function (`COUNT`, `SUM(field)`, etc.) |
| `group_by_state()` | -- | Group results by current state |
| `group_by_field(field)` | `&str` | Group results by a data field |
