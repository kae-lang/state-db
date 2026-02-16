# Error Handling

All SDK methods return `SdkResult<T>`, which is an alias for `Result<T, SdkError>`. This page covers every error variant, when it occurs, and how to handle it.

## SdkResult

```rust
pub type SdkResult<T> = Result<T, SdkError>;
```

Every async method on `SmqlClient`, `FindBuilder`, `AggregateBuilder`, and `Subscription` returns `SdkResult<T>`.

## SdkError Variants

```rust
pub enum SdkError {
    Http(reqwest::Error),
    Server(String),
    TransitionDenied(String),
    NotFound(String),
    Parse(String),
    Subscription(String),
    Deserialize(String),
    InvalidUrl(String),
}
```

### `Http`

Wraps a `reqwest::Error`. Occurs when the HTTP request itself fails -- network unreachable, DNS resolution failure, connection refused, or timeout.

```rust
match client.health().await {
    Err(SdkError::Http(e)) => {
        eprintln!("Network error: {}", e);
        if e.is_timeout() {
            eprintln!("Request timed out");
        }
        if e.is_connect() {
            eprintln!("Could not connect to server");
        }
    }
    _ => {}
}
```

### `Server`

The server returned an error response that does not fall into a more specific category. The string contains the error message from the server.

```rust
match client.execute("INVALID SMQL").await {
    Err(SdkError::Server(msg)) => {
        eprintln!("Server error: {}", msg);
    }
    _ => {}
}
```

Common causes:
- Syntax errors in SMQL statements
- Schema validation failures
- Internal server errors

### `TransitionDenied`

The server rejected a transition because a guard condition was not met, the transition is not allowed from the current state, or a `BEFORE` hook rejected it.

```rust
use smql_sdk::TransitionOptions;

match client.transition("01HQXYZ...", "shipped", TransitionOptions::default()).await {
    Ok(result) => println!("Shipped!"),
    Err(SdkError::TransitionDenied(reason)) => {
        println!("Cannot ship: {}", reason);
    }
    Err(e) => return Err(e.into()),
}
```

This is the error that `try_transition` catches and converts to `Ok(None)`:

```rust
// These are equivalent:
let result = client.try_transition(id, "shipped", opts).await?;

// vs.
let result = match client.transition(id, "shipped", opts).await {
    Ok(r) => Some(r),
    Err(SdkError::TransitionDenied(_)) => None,
    Err(e) => return Err(e),
};
```

### `NotFound`

The requested resource does not exist. Returned when getting an instance or machine that does not exist.

```rust
match client.get_instance("nonexistent-id").await {
    Err(SdkError::NotFound(msg)) => {
        println!("Not found: {}", msg);
    }
    _ => {}
}
```

```rust
match client.get_machine("NoSuchMachine").await {
    Err(SdkError::NotFound(msg)) => {
        println!("Machine not found: {}", msg);
    }
    _ => {}
}
```

### `Parse`

The server returned a response that the SDK could not parse as the expected JSON structure. This usually indicates a version mismatch between the SDK and the server.

```rust
match client.list_machines().await {
    Err(SdkError::Parse(msg)) => {
        eprintln!("Unexpected response format: {}", msg);
    }
    _ => {}
}
```

### `Subscription`

An error related to the WebSocket subscription. Occurs when the WebSocket connection fails to open, is dropped, or receives an unparseable message.

```rust
match sub.next_event().await {
    Err(SdkError::Subscription(msg)) => {
        eprintln!("Subscription error: {}", msg);
        // Typically means the connection was lost
    }
    _ => {}
}
```

### `Deserialize`

JSON deserialization failed. Most commonly encountered when using `TypedInstance::try_from` and the instance data does not match the expected data struct.

```rust
use smql_sdk::TypedInstance;

let response = client.get_instance("01HQXYZ...").await?;
match TypedInstance::<Order>::try_from(response) {
    Err(SdkError::Deserialize(msg)) => {
        eprintln!("Data shape mismatch: {}", msg);
    }
    Ok(typed) => {
        println!("Order item: {}", typed.data.item);
    }
    _ => {}
}
```

### `InvalidUrl`

The URL passed to `SmqlClient::new` or `SmqlClient::builder` is not a valid URL.

```rust
match SmqlClient::new("not a url") {
    Err(SdkError::InvalidUrl(msg)) => {
        eprintln!("Bad URL: {}", msg);
    }
    _ => {}
}
```

## Pattern Matching

### Comprehensive Match

Handle every variant explicitly:

```rust
use smql_sdk::SdkError;

match client.spawn("Order", data).await {
    Ok(instance) => {
        println!("Created: {}", instance.id);
    }
    Err(SdkError::Http(e)) => eprintln!("Network: {}", e),
    Err(SdkError::Server(msg)) => eprintln!("Server: {}", msg),
    Err(SdkError::NotFound(msg)) => eprintln!("Not found: {}", msg),
    Err(SdkError::TransitionDenied(msg)) => eprintln!("Denied: {}", msg),
    Err(SdkError::Parse(msg)) => eprintln!("Parse: {}", msg),
    Err(SdkError::Deserialize(msg)) => eprintln!("Deserialize: {}", msg),
    Err(SdkError::Subscription(msg)) => eprintln!("Subscription: {}", msg),
    Err(SdkError::InvalidUrl(msg)) => eprintln!("Invalid URL: {}", msg),
}
```

### Grouping Retryable Errors

```rust
fn is_retryable(err: &SdkError) -> bool {
    match err {
        SdkError::Http(e) => e.is_timeout() || e.is_connect(),
        SdkError::Subscription(_) => true,
        _ => false,
    }
}
```

### Retry Loop

```rust
use smql_sdk::{SmqlClient, SdkError};
use std::time::Duration;

async fn spawn_with_retry(
    client: &SmqlClient,
    machine: &str,
    data: serde_json::Value,
    max_retries: u32,
) -> smql_sdk::SdkResult<smql_sdk::InstanceResponse> {
    let mut attempts = 0;
    loop {
        match client.spawn(machine, data.clone()).await {
            Ok(instance) => return Ok(instance),
            Err(e) if is_retryable(&e) && attempts < max_retries => {
                attempts += 1;
                let delay = Duration::from_millis(100 * 2u64.pow(attempts));
                eprintln!("Attempt {} failed, retrying in {:?}...", attempts, delay);
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
}
```

## Using the `?` Operator

`SdkError` works with the `?` operator in functions that return `SdkResult<T>` or any `Result` type where the error implements `From<SdkError>`.

```rust
async fn process_order(client: &SmqlClient) -> smql_sdk::SdkResult<()> {
    let order = client.spawn("Order", serde_json::json!({
        "item": "Widget",
        "quantity": 5,
        "total": [2500, "USD"],
    })).await?;

    client.transition(
        &order.id,
        "confirmed",
        smql_sdk::TransitionOptions::default(),
    ).await?;

    Ok(())
}
```

For application code using `Box<dyn std::error::Error>`:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SmqlClient::new("http://localhost:3000")?;

    let machines = client.list_machines().await?;
    println!("Machines: {:?}", machines);

    Ok(())
}
```

## Display and Debug

`SdkError` implements both `Display` and `Debug`. The `Display` implementation provides human-readable messages:

```rust
if let Err(e) = client.health().await {
    // Display -- user-friendly
    println!("Error: {}", e);

    // Debug -- includes internal details
    println!("Debug: {:?}", e);
}
```
