# SMQL Engine

A State Machine Query Language database engine in Rust. Define state machines, spawn instances, transition between states, and query your data — all through a declarative language.

## Quick Start

### Define a Machine

```smql
DEFINE MACHINE SupportTicket (
    DATA {
        subject : TEXT -> REQUIRED
        priority : ENUM(low, medium, high) -> DEFAULT(medium)
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

### Use the CLI

```bash
# Execute SMQL via HTTP API
curl -X POST http://localhost:4200/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "SPAWN SupportTicket { subject: \"Bug report\", priority: \"high\" }"}'
```

### Use the SDK

```rust
use smql_sdk::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = SmqlClient::new("http://localhost:4200")?;

    // Define a machine
    client.define_machine(r#"
        DEFINE MACHINE Counter (
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
    let inst = client.spawn("Counter", serde_json::json!({})).await?;
    println!("Spawned: {} (state: {})", inst.id, inst.state);

    // Transition
    let tr = client
        .transition(&inst.id, "running", TransitionOptions::default())
        .await?;
    println!("Transitioned: {} -> {}", tr.from_state, tr.to_state);

    // Query
    let results = client.find("Counter").in_state("running").execute().await?;
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
    smql-engine/       # Core engine (spawn, transition, query, expression eval)
    smql-trail/        # Trail (audit log) types and storage
    smql-timer/        # Timeout/dwell timer management with persistence
    smql-hooks/        # Hook execution + EventBus (tokio::broadcast)
    smql-query/        # Query planner and execution
    smql-server/       # HTTP/JSON API (axum) + WebSocket + auth middleware
    smql-cli/          # CLI binary + REPL (rustyline)
    smql-sdk/          # Client SDK library (reqwest + builders)
    smql-codegen/      # Code generator (.smql -> Rust)
```

## Features

- **State machines**: Define states, transitions, guards, mutations, timeouts
- **Composition**: Parent-child machine relationships with CASCADE, SIGNAL PARENT TO, SPAWN in MUTATE
- **Hooks**: ON SPAWN, BEFORE/AFTER EACH TRANSITION, ON ENTER/EXIT state, EMIT events
- **Queries**: FIND (with cursor pagination), GET, TRAIL, AGGREGATE, PATHS, FUNNEL, COMPARE PATHS
- **Schema evolution**: ALTER MACHINE with live migration (ADD/REMOVE states, transitions, data fields)
- **Storage backends**: In-memory (default) or RocksDB (feature-gated)
- **Observability**: Prometheus metrics, WebSocket event streaming, structured logging (tracing)
- **Webhooks**: HTTP POST actions with retry on 5xx/network errors
- **Auth**: JWT HS256 middleware (feature-gated behind `auth`)
- **Timer persistence**: Write-through to storage, automatic restore on startup
- **SDK**: Ergonomic Rust client with builders, typed instances, WebSocket subscriptions
- **Codegen**: Generate typed Rust code from `.smql` definitions

## HTTP API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/execute` | Execute any SMQL statement (`{"smql": "..."}`) |
| `GET` | `/machines` | List all registered machines |
| `GET` | `/machines/:name` | Get machine definition |
| `GET` | `/instances/:id` | Get instance by ULID |
| `GET` | `/health` | Health check |
| `GET` | `/metrics` | Prometheus metrics |
| `GET` | `/subscribe` | WebSocket event stream |

## CLI Commands

```
smql serve    --bind 127.0.0.1:4200 --storage memory
smql repl     --storage memory
smql exec     "SPAWN Counter {}"
smql run      script.smql
smql codegen  --input machines/ --output src/generated/
```

## REPL

```
smql> FIND SupportTicket WHERE STATE IS open LIMIT 3
3 results (12ms)

smql> .machines
SupportTicket, Order, Pipeline

smql> .states SupportTicket
open, triaged, in_progress, waiting_on_customer, resolved, closed, reopened

smql> .transitions SupportTicket
open -> triaged, triaged -> in_progress, ...
```

Meta-commands: `.help`, `.machines`, `.states <machine>`, `.transitions <machine>`

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
# All tests (932 tests)
cargo test

# With RocksDB (44 additional tests)
cargo test --features rocksdb

# With auth (11 additional tests)
cargo test --features auth

# Specific crate
cargo test -p smql-sdk
cargo test -p smql-codegen
cargo test -p smql-engine-core
```

## Language Reference

See [smql-guide.md](../smql-guide.md) for the complete SMQL language specification and developer guide.
