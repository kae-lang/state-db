# Getting Started with the SMQL SDK

The SMQL SDK provides a type-safe Rust client for interacting with an SMQL Engine server. It wraps the HTTP and WebSocket APIs into an ergonomic, async interface.

## Installation

Add `smql-sdk` to your `Cargo.toml`:

```toml
[dependencies]
smql-sdk = { path = "../smql-engine/smql-sdk" }
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

## Creating a Client

The simplest way to create a client is with `SmqlClient::new`:

```rust
use smql_sdk::SmqlClient;

let client = SmqlClient::new("http://localhost:3000")?;
```

For more control, use the builder:

```rust
use smql_sdk::SmqlClient;
use std::time::Duration;

let client = SmqlClient::builder("http://localhost:3000")
    .timeout(Duration::from_secs(30))
    .build()?;
```

## Basic Usage

### Define a Machine

```rust
let result = client.define_machine(r#"
    MACHINE SupportTicket {
        STATE open {
            ON_ENTER { REQUIRE subject: TEXT, priority: INT }
        }
        STATE in_progress
        STATE resolved
        STATE closed TERMINAL

        open -> in_progress
        in_progress -> resolved
        resolved -> closed
        resolved -> open
    }
"#).await?;

println!("Action: {}", result.action);
```

### Spawn an Instance

```rust
use serde_json::json;

let instance = client.spawn("SupportTicket", json!({
    "subject": "Login page broken",
    "priority": 1
})).await?;

println!("Created instance: {}", instance.id);
println!("Current state: {}", instance.state);
```

### Transition an Instance

```rust
use smql_sdk::TransitionOptions;

let response = client.transition(
    &instance.id,
    "in_progress",
    TransitionOptions::default(),
).await?;

println!("Moved from {} to {}", response.from_state, response.to_state);
```

### Query Instances

```rust
let tickets = client.find("SupportTicket")
    .in_state("open")
    .where_clause("priority == 1")
    .limit(10)
    .execute()
    .await?;

for ticket in &tickets {
    println!("{}: {}", ticket.id, ticket.data["subject"]);
}
```

### Check Server Health

```rust
let healthy = client.health().await?;
println!("Server healthy: {}", healthy);
```

## Complete Example

Here is a full working example that defines a machine, spawns an instance, transitions it through states, and queries the trail:

```rust
use smql_sdk::{SmqlClient, TransitionOptions};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SmqlClient::new("http://localhost:3000")?;

    // Define the machine
    client.define_machine(r#"
        MACHINE Order {
            STATE pending {
                ON_ENTER { REQUIRE item: TEXT, quantity: INT }
            }
            STATE confirmed
            STATE shipped
            STATE delivered TERMINAL
            STATE cancelled TERMINAL

            pending -> confirmed
            confirmed -> shipped
            shipped -> delivered
            pending -> cancelled
            confirmed -> cancelled
        }
    "#).await?;

    // Spawn an order
    let order = client.spawn("Order", json!({
        "item": "Mechanical Keyboard",
        "quantity": 2
    })).await?;

    println!("Order {} created in state '{}'", order.id, order.state);

    // Move through the workflow
    client.transition(&order.id, "confirmed", TransitionOptions::default()).await?;
    client.transition(&order.id, "shipped", TransitionOptions::default()).await?;
    client.transition(&order.id, "delivered", TransitionOptions::default()).await?;

    // Check the audit trail
    let trail = client.trail(&order.id).await?;
    for entry in &trail {
        println!("[{}] {} -> {} ({})",
            entry.sequence, entry.from_state, entry.to_state, entry.timestamp);
    }

    Ok(())
}
```

## Next Steps

- [Client API Reference](./client-api) -- full method documentation
- [Queries](./queries) -- FindBuilder and AggregateBuilder
- [WebSocket Subscriptions](./websocket-subscriptions) -- real-time events
- [Typed API](./typed-api) -- code-generated type-safe wrappers
- [Error Handling](./error-handling) -- SdkError patterns
