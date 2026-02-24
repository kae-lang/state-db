# WebSocket Subscriptions

The SDK supports real-time event streaming over WebSocket. You can subscribe to all engine events or filter to a specific machine.

## Opening a Subscription

Use `client.subscribe()` to create a `SmqlSubscription`, then call `.connect()` to open the WebSocket.

### Subscribe to All Events

```typescript
const sub = client.subscribe();
await sub.connect();
```

### Subscribe to a Specific Machine

```typescript
const sub = client.subscribe({ machine: "Order" });
await sub.connect();
```

### Subscribe to a Specific Event Type

```typescript
const sub = client.subscribe({ machine: "Order", event: "transitioned" });
await sub.connect();
```

## Receiving Events

### Filtered by Event Name

```typescript
const unsub = sub.on("transitioned", (event) => {
  console.log(`${event.machine} instance ${event.instance_id} changed state`);
});

// Later, remove this handler
unsub();
```

### All Events

```typescript
const unsub = sub.onAny((event) => {
  console.log(`[${event.event}] ${event.machine}: ${event.instance_id}`);
});
```

Both `.on()` and `.onAny()` return an unsubscribe function.

## SubscriptionEvent

Every event has this shape:

| Field | Type | Description |
|-------|------|-------------|
| `event` | `string` | Event name (`"spawned"`, `"transitioned"`, `"timeout"`, or custom EMIT names) |
| `machine` | `string` | Machine the event relates to |
| `instance_id` | `string?` | Instance ULID, if applicable |
| `actor` | `string?` | The actor who caused the event |
| `data` | `unknown?` | Additional event payload |

### Common Event Names

| Event | When It Fires |
|-------|---------------|
| `spawned` | A new instance was created |
| `transitioned` | An instance changed state |
| `timeout` | A timeout transition fired |
| Custom names | Emitted by `EMIT("name")` in hooks |

## Closing a Subscription

```typescript
sub.close();
console.log(sub.connected); // false
```

## Connection Status

```typescript
if (sub.connected) {
  console.log("WebSocket is open");
}
```

## Authentication

When the client has a `token` configured, it is passed as a `?token=` query parameter on the WebSocket URL (since WebSocket doesn't support custom headers in browsers).

## Practical Example

A monitoring service that logs all transitions and alerts on timeouts:

```typescript
import { SmqlClient } from "smql-sdk";

async function monitor() {
  const client = new SmqlClient({ url: "http://localhost:4200" });
  const sub = client.subscribe();
  await sub.connect();

  sub.on("transitioned", (event) => {
    console.log(
      `[TRANSITION] ${event.machine} ${event.instance_id} by ${event.actor ?? "system"}`
    );
  });

  sub.on("timeout", (event) => {
    console.error(
      `[TIMEOUT] ${event.machine} ${event.instance_id} timed out!`
    );
  });

  sub.on("spawned", (event) => {
    console.log(`[SPAWN] ${event.machine} ${event.instance_id}`);
  });

  // Keep running until interrupted
  process.on("SIGINT", () => {
    sub.close();
    process.exit(0);
  });
}

monitor().catch(console.error);
```

## Reconnection

The `SmqlSubscription` does not auto-reconnect. For production use, wrap in a reconnect loop:

```typescript
async function subscribeWithReconnect(client: SmqlClient) {
  while (true) {
    try {
      const sub = client.subscribe({ machine: "Order" });
      await sub.connect();

      sub.onAny((event) => handleEvent(event));

      // Wait until disconnected
      await new Promise<void>((resolve) => {
        const check = setInterval(() => {
          if (!sub.connected) {
            clearInterval(check);
            resolve();
          }
        }, 1000);
      });

      console.log("Disconnected, reconnecting...");
    } catch (err) {
      console.error("Connection failed:", err);
      await new Promise((r) => setTimeout(r, 5000));
    }
  }
}
```
