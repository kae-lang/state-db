# Client API Reference

The `SmqlClient` is the primary entry point for interacting with an SMQL Engine server. All methods are async and return `SdkResult<T>`.

## Construction

### `SmqlClient::new`

Create a client with default settings.

```rust
use smql_sdk::SmqlClient;

let client = SmqlClient::new("http://localhost:3000")?;
```

Returns `SdkResult<SmqlClient>`. Fails with `SdkError::InvalidUrl` if the URL is malformed.

### `SmqlClient::builder`

Create a client with custom configuration.

```rust
use smql_sdk::SmqlClient;
use std::time::Duration;

let client = SmqlClient::builder("http://localhost:3000")
    .timeout(Duration::from_secs(60))
    .build()?;
```

#### SmqlClientBuilder Methods

| Method | Description |
|--------|-------------|
| `timeout(Duration)` | Set the HTTP request timeout |
| `build()` | Consume the builder and produce an `SdkResult<SmqlClient>` |

## Raw Execution

### `execute`

Send a raw SMQL string to the server. This is the lowest-level method; all other methods build on top of it.

```rust
let response = client.execute(r#"
    FIND Order WHERE state == "pending" LIMIT 5
"#).await?;

if response.success {
    println!("Result: {:?}", response.result);
} else {
    println!("Error: {:?}", response.error);
}
```

**Signature:**

```rust
pub async fn execute(&self, smql: &str) -> SdkResult<ExecuteResponse>
```

**Returns:** `ExecuteResponse` with fields:

| Field | Type | Description |
|-------|------|-------------|
| `success` | `bool` | Whether the statement executed without error |
| `result` | `Option<Value>` | The JSON result payload, if any |
| `error` | `Option<String>` | Error message on failure |
| `warnings` | `Option<Vec<String>>` | Non-fatal warnings from the engine |

## Machine Management

### `define_machine`

Define or update a state machine schema.

```rust
let result = client.define_machine(r#"
    MACHINE Invoice {
        STATE draft {
            ON_ENTER { REQUIRE amount: MONEY, customer: TEXT }
        }
        STATE sent
        STATE paid TERMINAL
        STATE void TERMINAL

        draft -> sent
        sent -> paid
        sent -> void
    }
"#).await?;

println!("{}", result.action); // "created" or "updated"
if let Some(warnings) = result.warnings {
    for w in &warnings {
        eprintln!("Warning: {}", w);
    }
}
```

**Signature:**

```rust
pub async fn define_machine(&self, smql: &str) -> SdkResult<DefineResult>
```

### `list_machines`

List all machine names registered on the server.

```rust
let machines = client.list_machines().await?;
for name in &machines {
    println!("Machine: {}", name);
}
```

**Signature:**

```rust
pub async fn list_machines(&self) -> SdkResult<Vec<String>>
```

### `get_machine`

Retrieve the schema details for a specific machine.

```rust
let info = client.get_machine("Invoice").await?;

println!("Machine: {}", info.name);
println!("Initial state: {}", info.initial_state);
println!("States: {:?}", info.states);
println!("Terminal states: {:?}", info.terminal_states);
println!("Schema version: {}", info.version);
```

**Signature:**

```rust
pub async fn get_machine(&self, name: &str) -> SdkResult<MachineInfo>
```

**Returns:** `MachineInfo` with fields:

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Machine name |
| `states` | `Vec<String>` | All state names |
| `initial_state` | `String` | The entry state |
| `terminal_states` | `Vec<String>` | States marked `TERMINAL` |
| `version` | `u64` | Schema version number |

## Instance Operations

### `spawn`

Create a new instance of a machine with initial data.

```rust
use serde_json::json;

let instance = client.spawn("Invoice", json!({
    "amount": [15000, "USD"],
    "customer": "Acme Corp"
})).await?;

println!("ID: {}", instance.id);
println!("State: {}", instance.state);
println!("Data: {}", instance.data);
```

**Signature:**

```rust
pub async fn spawn(
    &self,
    machine: &str,
    data: serde_json::Value,
) -> SdkResult<InstanceResponse>
```

Instance IDs are ULIDs (26-character, lexicographically sortable).

### `get_instance`

Fetch the current state of an instance by its ID.

```rust
let instance = client.get_instance("01HQXYZ...").await?;

println!("Machine: {}", instance.machine);
println!("State: {}", instance.state);
println!("Version: {}", instance.version);
println!("Data: {}", instance.data);
```

**Signature:**

```rust
pub async fn get_instance(&self, id: &str) -> SdkResult<InstanceResponse>
```

**Returns:** `InstanceResponse` with fields:

| Field | Type | Description |
|-------|------|-------------|
| `id` | `String` | Instance ULID |
| `machine` | `String` | Machine name |
| `state` | `String` | Current state |
| `data` | `Value` | Instance data as JSON |
| `created_at` | `String` | ISO 8601 creation timestamp |
| `updated_at` | `String` | ISO 8601 last update timestamp |
| `state_entered_at` | `String` | ISO 8601 timestamp of last state change |
| `trail_length` | `u64` | Number of audit trail entries |
| `version` | `u64` | Optimistic concurrency version |

### `transition`

Transition an instance to a new state. Fails if the transition is denied by a guard or not allowed by the schema.

```rust
use smql_sdk::TransitionOptions;

let result = client.transition(
    "01HQXYZ...",
    "sent",
    TransitionOptions::default(),
).await?;

println!("Transitioned from '{}' to '{}'", result.from_state, result.to_state);
```

With data, memo, and actor:

```rust
use smql_sdk::TransitionOptions;
use serde_json::json;

let opts = TransitionOptions {
    with_data: vec![
        ("tracking_number".to_string(), json!("TRK-12345")),
    ],
    memo: Some("Shipped via FedEx".to_string()),
    as_actor: Some("warehouse-bot".to_string()),
};

let result = client.transition("01HQXYZ...", "shipped", opts).await?;
```

**Signature:**

```rust
pub async fn transition(
    &self,
    instance_id: &str,
    to_state: &str,
    opts: TransitionOptions,
) -> SdkResult<TransitionResponse>
```

**Returns:** `TransitionResponse` with fields:

| Field | Type | Description |
|-------|------|-------------|
| `from_state` | `String` | The state before the transition |
| `to_state` | `String` | The state after the transition |
| `instance` | `InstanceResponse` | The updated instance |

### `try_transition`

Attempt a transition, returning `None` instead of an error if the transition is denied.

```rust
use smql_sdk::TransitionOptions;

match client.try_transition("01HQXYZ...", "paid", TransitionOptions::default()).await? {
    Some(result) => println!("Paid! Now in '{}'", result.to_state),
    None => println!("Transition denied — guard not satisfied"),
}
```

**Signature:**

```rust
pub async fn try_transition(
    &self,
    instance_id: &str,
    to_state: &str,
    opts: TransitionOptions,
) -> SdkResult<Option<TransitionResponse>>
```

Returns `Ok(None)` when the server returns `SdkError::TransitionDenied`. All other errors propagate normally.

### `trail`

Fetch the full audit trail for an instance.

```rust
let trail = client.trail("01HQXYZ...").await?;

for entry in &trail {
    println!("[seq {}] {} -> {}", entry.sequence, entry.from_state, entry.to_state);
    if let Some(actor) = &entry.actor {
        println!("  actor: {}", actor);
    }
    if let Some(memo) = &entry.memo {
        println!("  memo: {}", memo);
    }
    println!("  at: {}", entry.timestamp);
}
```

**Signature:**

```rust
pub async fn trail(&self, instance_id: &str) -> SdkResult<Vec<TrailEntryResponse>>
```

Sequence `0` is always the spawn event (with an empty `from_state`).

## Query Builders

### `find`

Start building a FIND query. See [Queries](./queries) for full details.

```rust
let results = client.find("Invoice")
    .in_state("sent")
    .where_clause("amount > 10000")
    .limit(20)
    .execute()
    .await?;
```

### `aggregate`

Start building an AGGREGATE query. See [Queries](./queries) for full details.

```rust
let result = client.aggregate("Invoice")
    .measure("COUNT")
    .group_by_state()
    .execute()
    .await?;
```

## Real-Time Events

### `subscribe`

Open a WebSocket subscription to real-time engine events. See [WebSocket Subscriptions](./websocket-subscriptions) for full details.

```rust
// Subscribe to all events
let mut sub = client.subscribe(None).await?;

// Subscribe to events for a specific machine
let mut sub = client.subscribe(Some("Invoice")).await?;
```

## Health Check

### `health`

Check if the server is reachable and healthy.

```rust
let ok = client.health().await?;
if !ok {
    eprintln!("Server is not healthy");
}
```

**Signature:**

```rust
pub async fn health(&self) -> SdkResult<bool>
```
