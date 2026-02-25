# Query Builders

The TypeScript SDK provides fluent builder classes for all SMQL query types. Each builder has a `.toSmql()` method for inspecting the generated SMQL and an `.execute()` method for running the query.

## FindBuilder

Find instances matching filters with sorting and pagination.

```typescript
const results = await client.find("SupportTicket")
  .where("priority == 1")
  .sortBy("created_at", "DESC")
  .limit(10)
  .execute();

console.log(`Found ${results.count} tickets`);
for (const ticket of results.instances) {
  console.log(`${ticket.id}: ${ticket.data.subject}`);
}
```

### Methods

| Method | Description |
|--------|-------------|
| `.select(...fields)` | Field projection -- only return these data fields |
| `.where(expr)` | Raw WHERE expression |
| `.inState(state)` | Shorthand for `WHERE STATE IS state` |
| `.stuckIn(state, duration)` | Shorthand for `WHERE STUCK IN state FOR duration` |
| `.sortBy(field, dir?)` | Add SORT BY clause (chainable, default ASC) |
| `.limit(n)` | Maximum results |
| `.offset(n)` | Skip first n results |
| `.after(id)` | Cursor-based pagination |
| `.asActor(role)` | AS ACTOR clause for role-based access |
| `.execute()` | Run the query, returns `Promise<FindResult>` |
| `.first()` | Returns first match or `null` |
| `.count()` | Returns total count |

### FindResult

```typescript
interface FindResult {
  count: number;
  instances: Instance[];
  next_cursor?: string;
}
```

### Examples

```typescript
// State filter shorthand
const open = await client.find("Ticket").inState("open").execute();

// Multiple sort clauses
const sorted = await client.find("Ticket")
  .sortBy("priority", "DESC")
  .sortBy("created_at", "ASC")
  .execute();

// First match
const latest = await client.find("Ticket")
  .sortBy("created_at", "DESC")
  .first();

// Cursor pagination
const page1 = await client.find("Ticket").limit(10).execute();
if (page1.next_cursor) {
  const page2 = await client.find("Ticket").limit(10).after(page1.next_cursor).execute();
}

// Stuck instances
const stale = await client.find("Order").stuckIn("pending", "24h").execute();

// Field projection -- only return subject and priority
const projected = await client.find("Ticket")
  .select("subject", "priority")
  .inState("open")
  .execute();
```

## AggregateBuilder

Compute aggregate measures over instances, optionally grouped.

```typescript
const stats = await client.aggregate("SupportTicket")
  .count("total")
  .avg("resolution_time", "avg_time")
  .groupByState()
  .execute();

for (const row of stats.rows) {
  console.log(`${row.group.state}: ${row.measures.total} tickets`);
}
```

### Methods

| Method | Description |
|--------|-------------|
| `.measure(func, field?, alias?)` | Generic measure |
| `.count(alias?)` | COUNT() |
| `.sum(field, alias?)` | SUM(field) |
| `.avg(field, alias?)` | AVG(field) |
| `.min(field, alias?)` | MIN(field) |
| `.max(field, alias?)` | MAX(field) |
| `.percentile(field, alias?)` | PERCENTILE(field) |
| `.where(expr)` | Filter before aggregating |
| `.groupByState()` | Group by current state |
| `.groupBy(field)` | Group by a data field |
| `.execute()` | Returns `Promise<AggregateResult>` |

### AggregateResult

```typescript
interface AggregateResult {
  rows: { group: Record<string, unknown>; measures: Record<string, unknown> }[];
}
```

## TrailBuilder

Fetch the audit trail (state change history) for an instance.

```typescript
const trail = await client.trail("01HQXYZ...").execute();

for (const entry of trail.entries) {
  console.log(`[${entry.sequence}] ${entry.from_state} -> ${entry.to_state}`);
  if (entry.actor) console.log(`  by: ${entry.actor}`);
  if (entry.memo) console.log(`  memo: ${entry.memo}`);
}
```

### Methods

| Method | Description |
|--------|-------------|
| `.byActor(actor)` | Filter by actor |
| `.fromState(state)` | Filter by source state |
| `.toState(state)` | Filter by target state |
| `.since(expr)` | Keep entries at or after this timestamp |
| `.until(expr)` | Keep entries at or before this timestamp |
| `.execute()` | Returns `Promise<TrailResult>` |

### TrailResult

```typescript
interface TrailResult {
  count: number;
  entries: TrailEntry[];
}

interface TrailEntry {
  sequence: number;
  from_state: string;
  to_state: string;
  actor?: string | null;
  memo?: string | null;
  timestamp: string;
}
```

## PathsBuilder

Analyze state transition paths taken by instances.

```typescript
const result = await client.paths("Order")
  .where("region == \"US\"")
  .limit(10)
  .execute();

for (const p of result.paths) {
  console.log(`${p.path.join(" -> ")}: ${p.count} instances`);
}
```

### Methods

| Method | Description |
|--------|-------------|
| `.where(expr)` | Filter instances |
| `.limit(n)` | Max paths to return |
| `.execute()` | Returns `Promise<PathsResult>` |

## FunnelBuilder

Measure conversion rates through a sequence of states.

```typescript
const result = await client.funnel("Order")
  .through(["pending", "confirmed", "shipped", "delivered"])
  .execute();

for (const stage of result.stages) {
  console.log(`${stage.state}: ${stage.count} (${(stage.conversion_rate * 100).toFixed(1)}%)`);
}
```

### Methods

| Method | Description |
|--------|-------------|
| `.through(states)` | Required state sequence |
| `.where(expr)` | Filter instances |
| `.execute()` | Returns `Promise<FunnelResult>` |

## ComparePathsBuilder

Compare transition paths across segments of a data field.

```typescript
const result = await client.comparePaths("Order")
  .segmentBy("region")
  .execute();

for (const segment of result.segments) {
  console.log(`Region: ${segment.segment_value}`);
  for (const p of segment.paths) {
    console.log(`  ${p.path.join(" -> ")}: ${p.count}`);
  }
}
```

### Methods

| Method | Description |
|--------|-------------|
| `.segmentBy(field)` | Required dimension to segment by |
| `.where(expr)` | Filter instances |
| `.execute()` | Returns `Promise<ComparePathsResult>` |

## ExplainTransitionsBuilder

Introspect which transitions are available for a machine or specific instance, including guard evaluation.

```typescript
// Schema-level: list all transitions defined on the machine
const schema = await client.explainTransitions("Order").execute();

for (const t of schema.transitions) {
  console.log(`${t.from_state} -> ${t.to_state} (guards: ${t.guards.join(", ")})`);
}

// Instance-level: evaluate guards against actual instance data
const available = await client.explainTransitions("Order")
  .instance("01HQXYZ...")
  .asActor("admin")
  .execute();

for (const t of available.transitions) {
  if (t.guards_met) {
    console.log(`Can transition to ${t.to_state}`);
  } else {
    console.log(`Blocked: ${t.blocking_guards.join(", ")}`);
  }
}
```

### Methods

| Method | Description |
|--------|-------------|
| `.instance(id)` | Evaluate against a specific instance |
| `.asActor(actor)` | Set the actor for guard evaluation |
| `.execute()` | Returns `Promise<ExplainTransitionsResult>` |

### REST Shortcut

```typescript
// Equivalent to EXPLAIN TRANSITIONS FOR Machine "id" AS actor
const result = await client.getTransitions("01HQXYZ...", "admin");
```

### ExplainTransitionsResult

```typescript
interface ExplainTransitionsResult {
  machine: string;
  current_state?: string;
  instance_id?: string;
  transitions: ExplainedTransition[];
}

interface ExplainedTransition {
  from_state: string;
  to_state: string;
  guards: string[];
  guards_met: boolean;
  blocking_guards: string[];
  recovery_options: RecoveryOption[];
  requires_data: string[];
  requires_role: string | null;
}
```

## GetEventsBuilder

Retrieve the durable event log with cursor-based pagination.

```typescript
// Get all events
const events = await client.getEvents().limit(100).execute();

for (const e of events.events) {
  console.log(`[${e.event_name}] ${e.machine} ${e.instance_id} at ${e.timestamp}`);
}

// Get events for a specific machine with pagination
const page1 = await client.getEvents("Order").limit(50).execute();
if (page1.next_cursor) {
  const page2 = await client.getEvents("Order")
    .after(page1.next_cursor)
    .limit(50)
    .execute();
}
```

### Methods

| Method | Description |
|--------|-------------|
| `.after(id)` | Cursor-based pagination (return events after this event ID) |
| `.limit(n)` | Maximum events to return |
| `.execute()` | Returns `Promise<EventsResult>` |

### EventsResult

```typescript
interface EventsResult {
  count: number;
  events: StoredEvent[];
  next_cursor?: string;
}

interface StoredEvent {
  id: string;
  timestamp: string;
  machine: string;
  event_name: string;
  instance_id?: string;
  payload?: unknown;
  actor?: string;
}
```

## Inspecting Generated SMQL

Every builder has a `.toSmql()` method for debugging:

```typescript
const builder = client.find("Order")
  .inState("pending")
  .sortBy("created_at", "DESC")
  .limit(10);

console.log(builder.toSmql());
// FIND Order WHERE STATE IS pending SORT BY created_at DESC LIMIT 10
```
