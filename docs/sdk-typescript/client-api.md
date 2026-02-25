# Client API Reference

The `SmqlClient` is the primary entry point for interacting with an SMQL Engine server. All async methods return typed Promises and throw typed errors on failure.

## Construction

```typescript
import { SmqlClient } from "smql-sdk";

const client = new SmqlClient({
  url: "http://localhost:4200",
  token: "optional-auth-token",
  timeout: 30_000,  // default: 30s
  headers: {},      // optional extra headers
});
```

### `SmqlClientConfig`

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `url` | `string` | *required* | Base URL of the SMQL server |
| `token` | `string?` | — | Bearer token for `Authorization` header |
| `timeout` | `number?` | `30000` | Request timeout in milliseconds |
| `headers` | `Record<string, string>?` | — | Additional HTTP headers |

## Raw Execution

### `execute`

Send a raw SMQL string to the server.

```typescript
const response = await client.execute('FIND Order WHERE STATE IS "pending" LIMIT 5');

if (response.success) {
  console.log("Result:", response.result);
}
```

**Signature:** `execute(smql: string): Promise<ExecuteResponse>`

**Returns:** `ExecuteResponse` with fields:

| Field | Type | Description |
|-------|------|-------------|
| `success` | `boolean` | Whether the statement succeeded |
| `result` | `unknown?` | The JSON result payload |
| `error` | `string?` | Error message on failure |
| `warnings` | `string[]?` | Non-fatal warnings |

### `executeAs<T>`

Execute and extract the typed result, throwing on failure.

```typescript
const instances = await client.executeAs<FindResult>("FIND Order LIMIT 10");
```

**Signature:** `executeAs<T>(smql: string): Promise<T>`

## Machine Management

### `defineMachine`

Returns a `DefineMachineBuilder`. See [Definition Builders](./definitions) for full details.

```typescript
await client.defineMachine("Invoice")
  .data("amount", "INT", "REQUIRED")
  .data("customer", "TEXT", "REQUIRED")
  .states("draft", "sent", "paid", "void")
  .initialState("draft")
  .terminalStates("paid", "void")
  .transition("draft", "sent").end()
  .transition("sent", "paid").end()
  .transition("sent", "void").end()
  .execute();
```

### `listMachines`

List all machine names registered on the server.

```typescript
const machines = await client.listMachines();
// ["Invoice", "Order", "SupportTicket"]
```

**Signature:** `listMachines(): Promise<string[]>`

### `getMachine`

Retrieve schema details for a specific machine.

```typescript
const info = await client.getMachine("Invoice");
console.log(info.name);            // "Invoice"
console.log(info.initial_state);   // "draft"
console.log(info.states);          // ["draft", "sent", "paid", "void"]
console.log(info.terminal_states); // ["paid", "void"]
console.log(info.version);         // 1
```

**Signature:** `getMachine(name: string): Promise<MachineInfo>`

## Instance Operations

### `spawn`

Returns a `SpawnBuilder`. Call `.execute()` to create the instance.

```typescript
const instance = await client.spawn("Invoice")
  .set({ amount: 15000, customer: "Acme Corp" })
  .execute();

console.log(instance.id);    // ULID
console.log(instance.state); // "draft"
```

With immediate transition:

```typescript
const instance = await client.spawn("Invoice")
  .set({ amount: 500, customer: "Widget Co" })
  .thenTransitionTo("sent")
  .execute();
// instance.state === "sent"
```

### `getInstance`

Fetch the current state of an instance by ID.

```typescript
const instance = await client.getInstance("01HQXYZ...");
```

**Signature:** `getInstance(id: string): Promise<Instance>`

**Returns:** `Instance` with fields:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Instance ULID |
| `machine` | `string` | Machine name |
| `state` | `string` | Current state |
| `data` | `Record<string, unknown>` | Instance data |
| `created_at` | `string` | ISO 8601 creation timestamp |
| `updated_at` | `string` | ISO 8601 last update timestamp |
| `state_entered_at` | `string` | When current state was entered |
| `trail_length` | `number` | Number of audit trail entries |
| `version` | `number` | Optimistic concurrency version |

### `deleteInstance`

Delete an instance by ID.

```typescript
const result = await client.deleteInstance("01HQXYZ...");
console.log(result.deleted); // true
```

**Signature:** `deleteInstance(id: string): Promise<DeleteInstanceResult>`

### `transition`

Returns a `TransitionBuilder`. Fails if the transition is denied.

```typescript
const result = await client.transition("Invoice", "01HQXYZ...", "sent")
  .execute();

console.log(`${result.from_state} -> ${result.to_state}`);
```

With data, memo, and actor:

```typescript
const result = await client.transition("Invoice", id, "shipped")
  .with({ tracking_number: "TRK-12345" })
  .memo("Shipped via FedEx")
  .asActor("warehouse-bot")
  .execute();
```

**Returns:** `TransitionResult` with fields:

| Field | Type | Description |
|-------|------|-------------|
| `from_state` | `string` | State before the transition |
| `to_state` | `string` | State after the transition |
| `instance` | `Instance` | The updated instance |

### `tryTransition`

Like `transition`, but returns a discriminated union instead of throwing on denial.

```typescript
const result = await client.tryTransition("Invoice", id, "paid")
  .execute();

if (result.transitioned) {
  console.log(`Paid! Now in '${result.to_state}'`);
} else {
  console.log("Transition denied — guard not satisfied");
}
```

**Returns:** `TryTransitionResult`:
- `{ transitioned: true, from_state, to_state, instance }` on success
- `{ transitioned: false }` when denied

### `transitionAll`

Batch transition all matching instances. Returns a `BatchTransitionBuilder`.

```typescript
const result = await client.transitionAll("SupportTicket")
  .where("STATE IS open")
  .to("closed")
  .memo("Bulk close")
  .execute();

console.log(`Matched: ${result.matched}, Transitioned: ${result.transitioned}`);
```

**Returns:** `BatchTransitionResult`

### `get`

Returns a `GetBuilder` for fetching a single instance with role-based access.

```typescript
const instance = await client.get("Invoice", id)
  .asActor("viewer")
  .execute();
```

### `trail`

Returns a `TrailBuilder` for fetching audit trail entries.

```typescript
const trail = await client.trail("01HQXYZ...").execute();

for (const entry of trail.entries) {
  console.log(`[${entry.sequence}] ${entry.from_state} -> ${entry.to_state}`);
}
```

## Query Builders

### `find`

Start building a FIND query. See [Query Builders](./queries) for full details.

```typescript
const results = await client.find("Invoice")
  .inState("sent")
  .sortBy("created_at", "DESC")
  .limit(20)
  .execute();
```

### `aggregate`

Start building an AGGREGATE query. See [Query Builders](./queries) for full details.

```typescript
const result = await client.aggregate("Invoice")
  .count()
  .groupByState()
  .execute();
```

### `paths` / `funnel` / `comparePaths`

Analytics query builders:

```typescript
const paths = await client.paths("Order").limit(10).execute();
const funnel = await client.funnel("Order").through(["pending", "confirmed", "shipped"]).execute();
const compared = await client.comparePaths("Order").segmentBy("region").execute();
```

### `explainTransitions`

Returns an `ExplainTransitionsBuilder`. Introspect available transitions for a machine or instance.

```typescript
// Schema-level
const schema = await client.explainTransitions("Order").execute();

// Instance-level with guard evaluation
const available = await client.explainTransitions("Order")
  .instance("01HQXYZ...")
  .asActor("admin")
  .execute();

for (const t of available.transitions) {
  console.log(`${t.from_state} -> ${t.to_state}: ${t.guards_met ? "allowed" : "blocked"}`);
}
```

### `getTransitions`

REST shortcut for explain transitions on a specific instance.

```typescript
const result = await client.getTransitions("01HQXYZ...", "admin");
```

**Signature:** `getTransitions(id: string, actor?: string): Promise<ExplainTransitionsResult>`

### `getEvents`

Returns a `GetEventsBuilder` for the durable event log.

```typescript
const events = await client.getEvents("Order").limit(100).execute();

for (const e of events.events) {
  console.log(`[${e.event_name}] ${e.instance_id}`);
}

// Paginate
if (events.next_cursor) {
  const more = await client.getEvents("Order")
    .after(events.next_cursor)
    .limit(100)
    .execute();
}
```

**Signature:** `getEvents(machine?: string): GetEventsBuilder`

## Definition Builders

### `definePolicy` / `defineView` / `defineProjection` / `defineRule` / `defineSubscription` / `defineSaga`

See [Definition Builders](./definitions) for full details on each.

### `alterMachine`

Returns an `AlterMachineBuilder` for schema migrations.

```typescript
const result = await client.alterMachine("Invoice")
  .addState("archived")
  .addTransition("void", "archived")
  .addData("archived_at", "DATETIME", ["OPTIONAL"])
  .execute();

console.log(`Version: ${result.new_version}, Migrated: ${result.instances_migrated}`);
```

## Views and Projections

### `getView` / `getProjection`

```typescript
const view = await client.getView("open_tickets");       // FindResult
const proj = await client.getProjection("ticket_stats");  // AggregateResult
```

## Real-Time Events

### `subscribe`

Create a WebSocket subscription. See [WebSocket Subscriptions](./websocket-subscriptions).

```typescript
const sub = client.subscribe({ machine: "Invoice" });
await sub.connect();

sub.on("transitioned", (event) => {
  console.log(`${event.machine} instance ${event.instance_id} changed state`);
});
```

## Health Check

```typescript
const ok = await client.health(); // true or throws
```

**Signature:** `health(): Promise<boolean>`

## Metrics

Fetch Prometheus-format metrics from the server.

```typescript
const metrics = await client.getMetrics(); // raw text
```

**Signature:** `getMetrics(): Promise<string>`
