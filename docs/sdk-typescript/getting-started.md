# Getting Started with the TypeScript SDK

The SMQL TypeScript SDK provides a fully-typed client for interacting with an SMQL Engine server from Node.js or any runtime with native `fetch` and `WebSocket` (Node 18+, Deno, Bun, Cloudflare Workers).

## Installation

```bash
npm install smql-sdk
```

Zero runtime dependencies. Requires Node.js 18+ (native `fetch`).

## Creating a Client

```typescript
import { SmqlClient } from "smql-sdk";

const client = new SmqlClient({ url: "http://localhost:4200" });
```

With authentication and custom timeout:

```typescript
const client = new SmqlClient({
  url: "http://localhost:4200",
  token: "my-api-token",
  timeout: 60_000, // 60 seconds
  headers: { "X-Request-Id": "abc" },
});
```

## Basic Usage

### Define a Machine

```typescript
await client.defineMachine("SupportTicket")
  .data("subject", "TEXT", "REQUIRED")
  .data("priority", "INT", { type: "MIN", value: 1 })
  .states("open", "in_progress", "resolved", "closed")
  .initialState("open")
  .terminalStates("closed")
  .transition("open", "in_progress").end()
  .transition("in_progress", "resolved").end()
  .transition("resolved", "closed").end()
  .transition("resolved", "open").end()
  .execute();
```

### Spawn an Instance

```typescript
const ticket = await client.spawn("SupportTicket")
  .set({ subject: "Login page broken", priority: 1 })
  .execute();

console.log("Created:", ticket.id);
console.log("State:", ticket.state);
```

### Transition an Instance

```typescript
const result = await client.transition("SupportTicket", ticket.id, "in_progress")
  .memo("Starting work")
  .asActor("engineer@example.com")
  .execute();

console.log(`${result.from_state} -> ${result.to_state}`);
```

### Query Instances

```typescript
const found = await client.find("SupportTicket")
  .inState("open")
  .sortBy("created_at", "DESC")
  .limit(10)
  .execute();

for (const t of found.instances) {
  console.log(`${t.id}: ${t.data.subject}`);
}
```

### Check Server Health

```typescript
const healthy = await client.health();
console.log("Server healthy:", healthy);
```

## Complete Example

```typescript
import { SmqlClient } from "smql-sdk";

async function main() {
  const client = new SmqlClient({ url: "http://localhost:4200" });

  // Define the machine
  await client.defineMachine("Order")
    .data("item", "TEXT", "REQUIRED")
    .data("quantity", "INT", { type: "MIN", value: 1 })
    .states("pending", "confirmed", "shipped", "delivered", "cancelled")
    .initialState("pending")
    .terminalStates("delivered", "cancelled")
    .transition("pending", "confirmed").end()
    .transition("confirmed", "shipped").end()
    .transition("shipped", "delivered").end()
    .transition("pending", "cancelled").end()
    .transition("confirmed", "cancelled").end()
    .execute();

  // Spawn an order
  const order = await client.spawn("Order")
    .set({ item: "Mechanical Keyboard", quantity: 2 })
    .execute();

  console.log(`Order ${order.id} created in '${order.state}'`);

  // Move through the workflow
  await client.transition("Order", order.id, "confirmed").execute();
  await client.transition("Order", order.id, "shipped").execute();
  await client.transition("Order", order.id, "delivered").execute();

  // Check the audit trail
  const trail = await client.trail(order.id).execute();
  for (const entry of trail.entries) {
    console.log(`[${entry.sequence}] ${entry.from_state} -> ${entry.to_state} (${entry.timestamp})`);
  }
}

main().catch(console.error);
```

## Next Steps

- [Client API Reference](./client-api) -- full method documentation
- [Query Builders](./queries) -- FindBuilder, AggregateBuilder, and more
- [WebSocket Subscriptions](./websocket-subscriptions) -- real-time events
- [Expression Builder](./expressions) -- programmatic filter construction
- [Error Handling](./error-handling) -- error classes and patterns
