# WebSocket Events

The SMQL server supports real-time event streaming over WebSocket. Clients connect to the `/subscribe` endpoint and receive JSON messages for every event emitted by the engine (spawns, transitions, hook `EMIT` calls, timeouts, etc.).

## Connecting

Upgrade an HTTP connection to WebSocket at the `/subscribe` path:

```
ws://127.0.0.1:4200/subscribe
```

### Filtering

Use query parameters to receive only a subset of events:

| Parameter | Description |
|-----------|-------------|
| `machine` | Only events for instances of this machine |
| `event` | Only events with this specific name |

Both parameters are optional and can be combined:

```
ws://127.0.0.1:4200/subscribe?machine=SupportTicket
ws://127.0.0.1:4200/subscribe?event=payment_received
ws://127.0.0.1:4200/subscribe?machine=Order&event=shipped
```

When no filters are set, the client receives all events from all machines.

## Event Message Format

Each message is a JSON object sent as a WebSocket text frame:

```json
{
  "event": "spawned",
  "instance_id": "01HXYZ1234567890ABCDEFGHIJ",
  "machine": "SupportTicket",
  "payload": null
}
```

| Field | Type | Description |
|-------|------|-------------|
| `event` | `string` | The event name (from `EMIT("name")` in hooks/actions, or `TIMEOUT` for timeout fires) |
| `instance_id` | `string` | ULID of the instance that produced the event |
| `machine` | `string` | Machine type of the instance |
| `payload` | `object \| null` | Optional data payload attached to the event |

### Event Sources

Events can originate from several places in a machine definition:

```sql
DEFINE MACHINE Order (
    STATES { pending, paid, shipped, delivered }
    INITIAL STATE pending
    TERMINAL STATES { delivered }
    TRANSITIONS {
        pending -> paid {
            ACTION : EMIT("payment_received")
        }
        paid -> shipped {}
        shipped -> delivered {}
    }
    HOOKS {
        ON SPAWN {
            EMIT("order_created")
        }
    }
)
```

Events from transition actions:
```json
{"event": "payment_received", "instance_id": "01H...", "machine": "Order", "payload": null}
```

Events from hooks:
```json
{"event": "order_created", "instance_id": "01H...", "machine": "Order", "payload": null}
```

Timeout events (fired by the timer system):
```json
{"event": "TIMEOUT", "instance_id": "01H...", "machine": "Order", "payload": null}
```

## Connection Lifecycle

- The server sends events as they occur. There is no initial backfill of historical events.
- If the client falls behind, the server logs a warning and skips missed events (broadcast channel lag behavior).
- The connection closes when the client sends a WebSocket Close frame or disconnects.
- If the server's EventBus shuts down, all WebSocket connections are closed.

## Client Examples

### curl / websocat

```bash
# All events
websocat ws://127.0.0.1:4200/subscribe

# Filtered to one machine
websocat "ws://127.0.0.1:4200/subscribe?machine=SupportTicket"
```

### JavaScript

```javascript
const ws = new WebSocket("ws://127.0.0.1:4200/subscribe?machine=Order");

ws.onmessage = (msg) => {
  const event = JSON.parse(msg.data);
  console.log(`[${event.machine}] ${event.event} on ${event.instance_id}`);
};

ws.onclose = () => {
  console.log("Disconnected, attempting reconnect...");
  setTimeout(() => connect(), 1000);
};
```

### Rust SDK

The SMQL SDK provides a typed subscription API:

```rust
use smql_sdk::SmqlClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SmqlClient::new("http://127.0.0.1:4200")?;

    // Subscribe to all events for the Order machine
    let mut sub = client.subscribe(Some("Order")).await?;

    // Poll events one at a time
    loop {
        let event = sub.next_event().await?;
        println!("Event: {} on {}", event.event, event.instance_id);
    }
}
```

Or use the callback-based API for background processing:

```rust
let sub = client.subscribe(None).await?;

let handle = sub.on_event(|event| {
    println!("[{}] {} on {}", event.machine, event.event, event.instance_id);
});

// ... do other work ...

// Cancel the subscription when done
// handle.cancel();
// Or let it cancel automatically when handle is dropped
```

## Reconnection

The server does not implement automatic reconnection. Clients should handle disconnects and reconnect with backoff:

1. Detect `onclose` or stream-end.
2. Wait with exponential backoff (e.g., 1s, 2s, 4s, up to 30s).
3. Reconnect to the same URL.
4. Resume processing. Note that events occurring during the disconnect are lost since there is no replay mechanism.

If your application requires guaranteed delivery, poll the `/execute` endpoint with `FIND` or `TRAIL` queries to catch up after reconnecting.
