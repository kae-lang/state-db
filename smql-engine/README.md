# SMQL Engine

A State Machine Query Language database engine in Rust. Define state machines, spawn instances, transition between states, and query your data — all through a declarative language.

## Quick Start

### Define a Machine

```smql
DEFINE MACHINE SupportTicket (
    DATA {
        subject : TEXT
        priority : ENUM(low, medium, high)
        assignee : TEXT -> OPTIONAL
    }
    STATES { open, assigned, in_progress, resolved, closed }
    INITIAL STATE open
    TERMINAL STATES { closed }
    TRANSITIONS {
        open -> assigned {}
        assigned -> in_progress {}
        in_progress -> resolved {}
        resolved -> closed {}
    }
)
```

### Start the Server

```bash
cargo run --bin smql -- serve --bind 127.0.0.1:4200
```

### Use the SDK

```rust
use smql_sdk::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SmqlClient::new("http://localhost:4200")?;

    // Define a machine
    client.define_machine(r#"
        DEFINE MACHINE counter (
            STATES { idle, running, done }
            INITIAL STATE idle
            TERMINAL STATES { done }
            TRANSITIONS {
                idle -> running {}
                running -> done {}
            }
        )
    "#).await?;

    // Spawn an instance
    let inst = client.spawn("counter", serde_json::json!({})).await?;
    println!("Spawned: {} (state: {})", inst.id, inst.state);

    // Transition
    let tr = client
        .transition(&inst.id, "running", TransitionOptions::default())
        .await?;
    println!("Transitioned: {} -> {}", tr.from_state, tr.to_state);

    // Query
    let results = client.find("counter").in_state("running").execute().await?;
    println!("Found {} running instances", results.len());

    Ok(())
}
```

### Generate Typed Code

```bash
cargo run --bin smql -- codegen --input machines/ --output src/generated/
```

This parses `.smql` files and generates typed Rust structs, state enums, and machine markers.

## Architecture

```
smql-engine/
  crates/
    smql-ast/          # AST types (MachineDefinition, Expression, Value, etc.)
    smql-parser/       # Hand-written recursive descent parser
    smql-catalog/      # Machine registry (DashMap-backed)
    smql-storage/      # Storage traits + MemoryStorage + RocksDB
    smql-engine/       # Core engine (spawn, transition, query execution)
    smql-timer/        # Timeout/dwell timer management
    smql-hooks/        # Hook execution + EventBus
    smql-query/        # Query planner and execution
    smql-server/       # HTTP/JSON API (axum) + WebSocket
    smql-cli/          # CLI binary + REPL
    smql-sdk/          # Client SDK library
    smql-codegen/      # Code generator (.smql -> Rust)
```

## Features

- **State machines**: Define states, transitions, guards, mutations, timeouts
- **Composition**: Parent-child machine relationships with CASCADE transitions
- **Hooks**: ON SPAWN, BEFORE/AFTER EACH TRANSITION, ON ENTER/EXIT, EMIT events
- **Queries**: FIND, GET, TRAIL, AGGREGATE, PATHS, FUNNEL
- **Schema evolution**: ALTER MACHINE with live migration
- **Storage backends**: In-memory (default) or RocksDB (feature-gated)
- **Observability**: Prometheus metrics, WebSocket event streaming, structured logging
- **SDK**: Ergonomic Rust client with builders, typed instances, WebSocket subscriptions
- **Codegen**: Generate typed Rust code from `.smql` definitions

## CLI Commands

```
smql serve    --bind 127.0.0.1:4200 --storage memory
smql repl     --storage memory
smql exec     "SPAWN counter {}"
smql run      script.smql
smql codegen  --input machines/ --output src/generated/
```

## SDK API

```rust
// Connection
let client = SmqlClient::new("http://localhost:4200")?;
let client = SmqlClient::builder("http://localhost:4200")
    .timeout(Duration::from_secs(5))
    .build()?;

// Machines
client.define_machine(smql).await?;
client.list_machines().await?;
client.get_machine("name").await?;

// Instances
client.spawn("machine", json!({"key": "val"})).await?;
client.get_instance("id").await?;
client.transition("id", "state", opts).await?;
client.try_transition("id", "state", opts).await?;
client.trail("id").await?;

// Queries
client.find("machine").in_state("open").limit(10).execute().await?;
client.aggregate("machine").measure("COUNT()").group_by_state().execute().await?;

// WebSocket
let mut sub = client.subscribe(Some("machine")).await?;
let event = sub.next_event().await?;

// Typed (with codegen)
let inst = client.spawn_typed::<MyMachine>(data).await?;
let results = client.find_typed::<MyMachine>().in_state("open").execute().await?;
```

## Running Tests

```bash
# All tests
cargo test

# With RocksDB
cargo test --features rocksdb

# Specific crate
cargo test -p smql-sdk
cargo test -p smql-codegen
```
