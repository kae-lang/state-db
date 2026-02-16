# Tutorial 6: Production Deployment

In the previous tutorials, you used in-memory storage and the REPL. For production, you need persistent storage, monitoring, programmatic access, and the ability to evolve your schema over time.

This tutorial covers the complete production setup.

## Step 1: Persistent Storage with RocksDB

In-memory storage is great for development but loses all data on restart. Switch to RocksDB for persistence:

```bash
smql serve --bind 0.0.0.0:4200 --storage ./data
```

When `--storage` points to a directory path, SMQL uses RocksDB. The directory is created automatically.

### Building with RocksDB Support

RocksDB is behind a feature flag. Build with it enabled:

```bash
cargo build --release --features rocksdb
```

### Storage Comparison

| Feature | Memory | RocksDB |
|---------|--------|---------|
| Persistence | No | Yes |
| Performance | Fastest | Fast |
| Concurrency | DashMap | Column families |
| Use case | Dev, testing | Production |

Both backends implement the same `Storage` trait, so your SMQL statements work identically with either backend.

## Step 2: Prometheus Metrics

SMQL exposes Prometheus metrics at `/metrics`:

```bash
curl http://localhost:4200/metrics
```

```
# HELP smql_instances_total Current number of instances by machine and state
# TYPE smql_instances_total gauge
smql_instances_total{machine="SupportTicket",state="open"} 15
smql_instances_total{machine="SupportTicket",state="in_progress"} 8

# HELP smql_transitions_total Total transitions by machine
# TYPE smql_transitions_total counter
smql_transitions_total{machine="SupportTicket",from="open",to="triaged"} 38

# HELP smql_transition_duration_seconds Transition execution time
# TYPE smql_transition_duration_seconds histogram
smql_transition_duration_seconds_bucket{machine="SupportTicket",le="0.001"} 42

# HELP smql_spawns_total Total spawns by machine
# TYPE smql_spawns_total counter
smql_spawns_total{machine="SupportTicket"} 43

# HELP smql_guard_failures_total Guard failures by machine
# TYPE smql_guard_failures_total counter
smql_guard_failures_total{machine="SupportTicket"} 7

# HELP smql_timeout_fires_total Timeout transitions fired
# TYPE smql_timeout_fires_total counter
smql_timeout_fires_total{machine="SupportTicket",state="waiting_on_customer"} 3

# HELP smql_query_duration_seconds Query execution time
# TYPE smql_query_duration_seconds histogram
smql_query_duration_seconds_bucket{query_type="find",le="0.01"} 156
```

### Available Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `smql_instances_total` | Gauge | machine, state | Current instances per state |
| `smql_transitions_total` | Counter | machine, from, to | Total transitions |
| `smql_transition_duration_seconds` | Histogram | machine | Transition execution time |
| `smql_spawns_total` | Counter | machine | Total spawns |
| `smql_guard_failures_total` | Counter | machine | Failed guard evaluations |
| `smql_timeout_fires_total` | Counter | machine, state | Timeout transitions |
| `smql_query_duration_seconds` | Histogram | query_type | Query execution time |

### Grafana Dashboard

Point Grafana at the `/metrics` endpoint and build dashboards for:
- Instance count by state (use `smql_instances_total`)
- Transition rate (use `rate(smql_transitions_total[5m])`)
- Guard failure rate (use `rate(smql_guard_failures_total[5m])`)
- Query latency percentiles (use `smql_query_duration_seconds`)

## Step 3: WebSocket Event Streaming

For real-time monitoring, connect to the WebSocket endpoint:

```bash
websocat ws://localhost:4200/subscribe
```

Filter by machine:

```bash
websocat "ws://localhost:4200/subscribe?machine=SupportTicket"
```

Events arrive as JSON:

```json
{"event":"ticket.assigned","machine":"SupportTicket","instance_id":"01JM...","actor":"agent_1"}
```

Use WebSocket for:
- Real-time dashboards
- Alerting pipelines
- Integration with external systems (Slack, PagerDuty, etc.)

## Step 4: Using the Rust SDK

For production applications, use the Rust SDK instead of curl.

### Add the Dependency

```toml
[dependencies]
smql-sdk = { path = "../smql-engine/crates/smql-sdk" }
tokio = { version = "1", features = ["full"] }
serde_json = "1"
```

### Client Setup

```rust
use smql_sdk::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SmqlClient::new("http://localhost:4200")?;

    // Health check
    assert!(client.health().await?);

    Ok(())
}
```

### Define Machines

```rust
let definition = r#"
    DEFINE MACHINE SupportTicket (
        DATA { subject: TEXT -> REQUIRED, priority: ENUM(low, medium, high) -> DEFAULT(medium) }
        STATES { open, closed }
        INITIAL STATE open
        TERMINAL STATES { closed }
        TRANSITIONS { open -> closed {} }
    )
"#;

client.define_machine(definition).await?;
```

### Spawn and Transition

```rust
// Spawn
let instance = client.spawn("SupportTicket", serde_json::json!({
    "subject": "Login broken"
})).await?;

println!("Created: {} in state {}", instance.id, instance.state);

// Transition
let result = client.transition(
    &instance.id,
    "closed",
    TransitionOptions::default(),
).await?;

println!("{} -> {}", result.from_state, result.to_state);
```

### Queries

```rust
// Find by state
let open_tickets = client.find("SupportTicket")
    .in_state("open")
    .sort_by("priority", "DESC")
    .limit(10)
    .execute()
    .await?;

// Count
let count = client.find("SupportTicket")
    .in_state("open")
    .count()
    .await?;

// Aggregate
let stats = client.aggregate("SupportTicket")
    .measure("COUNT()")
    .group_by_state()
    .execute()
    .await?;

// Trail
let trail = client.trail(&instance.id).await?;
for entry in &trail {
    println!("  {} -> {} (actor: {:?})", entry.from_state, entry.to_state, entry.actor);
}
```

### WebSocket Subscriptions

```rust
let mut sub = client.subscribe(Some("SupportTicket")).await?;

tokio::spawn(async move {
    loop {
        match sub.next_event().await {
            Ok(event) => println!("Event: {} on {}", event.event, event.instance_id),
            Err(e) => { eprintln!("Error: {}", e); break; }
        }
    }
});
```

### Error Handling

```rust
use smql_sdk::SdkError;

match client.transition(&id, "closed", TransitionOptions::default()).await {
    Ok(result) => println!("Transitioned to {}", result.to_state),
    Err(SdkError::NotFound(msg)) => println!("Instance not found: {}", msg),
    Err(SdkError::Validation(msg)) => println!("Guard failed: {}", msg),
    Err(SdkError::Timeout) => println!("Request timed out"),
    Err(e) => println!("Error: {:?}", e),
}
```

## Step 5: Code Generation

Generate type-safe Rust code from your machine definitions:

```bash
smql codegen --input machines/ --output src/generated/ --lang rust
```

This produces Rust structs for each machine's data and state enum:

```rust
// Auto-generated from SupportTicket machine definition

pub struct SupportTicketData {
    pub subject: String,
    pub priority: SupportTicketPriority,
}

pub enum SupportTicketPriority {
    Low,
    Medium,
    High,
}

pub enum SupportTicketState {
    Open,
    Closed,
}
```

Use the generated types with the SDK's typed API:

```rust
let instance = client.spawn_typed::<SupportTicket>(SupportTicketData {
    subject: "Login broken".into(),
    priority: SupportTicketPriority::High,
}).await?;
```

## Step 6: Schema Evolution with ALTER MACHINE

Production machines need to evolve. Use `ALTER MACHINE` to modify a machine definition without losing existing instances.

### Add a New State

```sql
ALTER MACHINE SupportTicket {
  ADD STATE escalated
  ADD TRANSITION in_progress -> escalated {}
  ADD TRANSITION escalated -> in_progress {}
}
```

### Add a New Data Field

```sql
ALTER MACHINE SupportTicket {
  ADD DATA severity : INT -> DEFAULT(1)
}
```

The `DEFAULT(1)` ensures existing instances get a value. Without a default, you must provide a `BACKFILL` expression:

```sql
ALTER MACHINE SupportTicket {
  ADD DATA severity : INT -> REQUIRED (BACKFILL 1)
}
```

### Remove a State

When removing a state, you must specify where existing instances should migrate:

```sql
ALTER MACHINE SupportTicket {
  REMOVE STATE triaged MIGRATE TO open
}
```

All instances currently in `triaged` will be moved to `open`. Transitions referencing `triaged` are cleaned up automatically.

### Multi-Operation ALTER

Combine multiple changes in a single ALTER:

```sql
ALTER MACHINE SupportTicket {
  ADD STATE escalated
  ADD DATA severity : INT -> DEFAULT(1)
  ADD TRANSITION in_progress -> escalated { GUARD : severity > 3 }
}
```

Operations are applied sequentially — later operations can reference changes made by earlier ones.

::: warning
ALTER MACHINE is a powerful operation. Test schema changes in a development environment before applying them to production.
:::

## Production Checklist

Before going to production, verify:

- [ ] **Storage**: Using RocksDB with a persistent directory
- [ ] **Monitoring**: Prometheus metrics endpoint connected to your monitoring stack
- [ ] **Alerts**: Set up alerts on `smql_guard_failures_total` and `smql_timeout_fires_total`
- [ ] **WebSocket**: Event consumers connected for real-time integration
- [ ] **SDK**: Application using the Rust SDK with proper error handling
- [ ] **Backup**: RocksDB data directory included in backup strategy
- [ ] **Schema**: Machine definitions version-controlled alongside application code
- [ ] **Testing**: Integration tests cover all transition paths and guard conditions

## What You Learned

| Concept | Summary |
|---------|---------|
| RocksDB | Persistent storage backend, enabled with `--storage ./path` |
| Feature flag | Build with `--features rocksdb` for RocksDB support |
| Prometheus | Metrics at `/metrics` for monitoring instances, transitions, latency |
| WebSocket | Real-time event streaming at `/subscribe` |
| SDK | Rust client with spawn, transition, query, and subscription APIs |
| Code generation | Generate type-safe Rust types from machine definitions |
| `ALTER MACHINE` | Evolve machine schema without losing existing data |
| `ADD STATE` / `REMOVE STATE` | Add new states or remove with migration |
| `ADD DATA` / `BACKFILL` | Add fields with defaults or backfill expressions |

## What's Next

Congratulations — you've completed the SMQL tutorial series. You now know how to:

1. Define state machines with states and transitions
2. Add typed data fields with validation constraints
3. Control access with guards and the ACTOR system
4. React to time with timeouts and to events with hooks
5. Build hierarchical machines with composition
6. Query and analyze your data with FIND, AGGREGATE, FUNNEL, and PATHS
7. Deploy to production with persistent storage, monitoring, and the SDK

For more detailed reference, explore:
- [Language Reference](/language/define-machine) — complete syntax documentation
- [HTTP API Reference](/server/http-api) — all server endpoints
- [SDK Reference](/sdk/client-api) — full SDK API documentation
- [Guides](/guides/support-ticket) — end-to-end real-world examples
