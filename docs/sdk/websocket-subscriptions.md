# WebSocket Subscriptions

The SDK supports real-time event streaming over WebSocket. You can subscribe to all engine events or filter to a specific machine, then process events as they arrive.

## Opening a Subscription

Use `client.subscribe` to open a WebSocket connection to the server.

### Subscribe to All Events

```rust
let mut sub = client.subscribe(None).await?;
```

### Subscribe to a Specific Machine

```rust
let mut sub = client.subscribe(Some("Order")).await?;
```

When a machine name is provided, only events related to that machine are delivered.

## Receiving Events

### Polling with `next_event`

Call `next_event` in a loop to process events one at a time. This method is async and will wait until the next event arrives.

```rust
let mut sub = client.subscribe(Some("Order")).await?;

loop {
    let event = sub.next_event().await?;
    println!("Event: {}", event.event);
    println!("Machine: {}", event.machine);

    if let Some(id) = &event.instance_id {
        println!("Instance: {}", id);
    }
    if let Some(actor) = &event.actor {
        println!("Actor: {}", actor);
    }
    if let Some(data) = &event.data {
        println!("Data: {}", data);
    }
}
```

**Signature:**

```rust
pub async fn next_event(&mut self) -> SdkResult<SdkEvent>
```

Returns `SdkError::Subscription` if the connection is closed or a parse error occurs.

### Callback with `on_event`

Use `on_event` to spawn a background task that invokes a callback for every event. This consumes the `Subscription` and returns a `SubscriptionHandle`.

```rust
let sub = client.subscribe(Some("SupportTicket")).await?;

let handle = sub.on_event(|event| {
    println!("[{}] {} — instance {:?}",
        event.machine, event.event, event.instance_id);
});

// The callback runs in the background.
// Do other work here...
tokio::time::sleep(std::time::Duration::from_secs(60)).await;

// When done, cancel the subscription.
// handle.cancel();
```

**Signature:**

```rust
pub fn on_event<F>(self, callback: F) -> SubscriptionHandle
where
    F: Fn(SdkEvent) + Send + Sync + 'static,
```

The callback closure must be `Send + Sync + 'static` because it runs on a spawned tokio task.

## Cancelling a Subscription

The `SubscriptionHandle` returned by `on_event` has a `cancel` method that stops the background listener.

```rust
let sub = client.subscribe(None).await?;

let mut handle = sub.on_event(|event| {
    println!("{}: {}", event.machine, event.event);
});

// Later, shut it down
handle.cancel();
```

**Signature:**

```rust
pub fn cancel(&mut self)
```

Calling `cancel` sends a signal to the background task, which closes the WebSocket connection and exits. Calling `cancel` more than once is a no-op.

If you drop the `SubscriptionHandle` without calling `cancel`, the background task will continue running until the WebSocket connection is closed by the server or an error occurs.

## SdkEvent

Every event delivered through a subscription is an `SdkEvent`.

| Field | Type | Description |
|-------|------|-------------|
| `event` | `String` | Event name (e.g., `"spawned"`, `"transitioned"`, or a custom EMIT name) |
| `machine` | `String` | Machine the event relates to |
| `instance_id` | `Option<String>` | Instance ULID, if applicable |
| `actor` | `Option<String>` | The actor who caused the event, if any |
| `data` | `Option<Value>` | Additional event payload as JSON |

### Common Event Names

| Event | When It Fires |
|-------|---------------|
| `spawned` | A new instance was created |
| `transitioned` | An instance changed state |
| `timeout` | A timeout transition fired |
| Custom names | Emitted by `EMIT("name")` in hooks |

## Practical Example

A monitoring service that logs all transitions and alerts on timeouts:

```rust
use smql_sdk::SmqlClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SmqlClient::new("http://localhost:3000")?;
    let sub = client.subscribe(None).await?;

    let mut handle = sub.on_event(|event| {
        match event.event.as_str() {
            "transitioned" => {
                println!(
                    "[TRANSITION] {} instance {} by {:?}",
                    event.machine,
                    event.instance_id.as_deref().unwrap_or("unknown"),
                    event.actor,
                );
            }
            "timeout" => {
                eprintln!(
                    "[TIMEOUT] {} instance {} timed out!",
                    event.machine,
                    event.instance_id.as_deref().unwrap_or("unknown"),
                );
            }
            "spawned" => {
                println!(
                    "[SPAWN] {} instance {}",
                    event.machine,
                    event.instance_id.as_deref().unwrap_or("unknown"),
                );
            }
            other => {
                println!(
                    "[EVENT] {} on {} — {:?}",
                    other, event.machine, event.instance_id,
                );
            }
        }
    });

    // Run until Ctrl+C
    tokio::signal::ctrl_c().await?;
    handle.cancel();
    Ok(())
}
```

## Error Handling

WebSocket-related errors are surfaced as `SdkError::Subscription`. Common failure scenarios:

- The server is not reachable when `subscribe` is called.
- The WebSocket connection drops mid-stream (network failure, server restart).
- The server sends a message that cannot be parsed as `SdkEvent`.

```rust
match sub.next_event().await {
    Ok(event) => handle_event(event),
    Err(smql_sdk::SdkError::Subscription(msg)) => {
        eprintln!("Subscription error: {}", msg);
        // Reconnect logic here
    }
    Err(e) => return Err(e.into()),
}
```

For long-lived services, consider wrapping the subscription in a reconnect loop:

```rust
loop {
    match client.subscribe(Some("Order")).await {
        Ok(mut sub) => {
            loop {
                match sub.next_event().await {
                    Ok(event) => handle_event(event),
                    Err(_) => {
                        eprintln!("Connection lost, reconnecting...");
                        break;
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to connect: {}", e);
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    }
}
```
