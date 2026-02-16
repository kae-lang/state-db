# SMQL Engine

A State Machine Query Language database engine in Rust. Define state machines, spawn instances, transition between states, and query your data — all through a declarative language.

## Installation

### Install Script (recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/kae-lang/state-db/main/install.sh | sh
```

Set a custom install directory:

```bash
SMQL_INSTALL_DIR=~/.local/bin curl -fsSL https://raw.githubusercontent.com/kae-lang/state-db/main/install.sh | sh
```

### Prebuilt Binaries

Download from [GitHub Releases](https://github.com/kae-lang/state-db/releases/latest):

| Target | Archive |
|--------|---------|
| macOS (Apple Silicon) | `smql-{version}-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `smql-{version}-x86_64-apple-darwin.tar.gz` |
| Linux x86_64 (static) | `smql-{version}-x86_64-unknown-linux-musl.tar.gz` |
| Linux aarch64 (static) | `smql-{version}-aarch64-unknown-linux-musl.tar.gz` |
| Windows x86_64 | `smql-{version}-x86_64-pc-windows-msvc.zip` |

All prebuilt binaries include RocksDB and JWT auth support.

### Docker

```bash
docker build -t smql .
docker run -p 4200:4200 -v smql-data:/data smql
```

The container runs `smql serve` on port 4200 with RocksDB storage at `/data/smql.db` by default.

### Build from Source

Requires Rust 1.89+ and a C++ compiler (for RocksDB).

```bash
git clone https://github.com/kae-lang/state-db.git
cd state-db/smql-engine
cargo build --release --bin smql --features "rocksdb,auth"
# Binary at target/release/smql
```

## Getting Started

### 1. Start the Server

```bash
# In-memory storage (data lost on restart)
smql serve

# With RocksDB persistent storage
smql serve --storage /path/to/smql.db

# Custom bind address
smql serve --bind 0.0.0.0:8080 --storage ./data.db
```

### 2. Define a Machine

Save this as `ticket.smql`:

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

Load it into the server:

```bash
smql run ticket.smql
```

Or via the HTTP API:

```bash
curl -X POST http://localhost:4200/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "DEFINE MACHINE SupportTicket ( STATES { open, closed } INITIAL STATE open TERMINAL STATES { closed } TRANSITIONS { open -> closed {} } )"}'
```

### 3. Spawn Instances and Transition

```bash
# Spawn
curl -s -X POST http://localhost:4200/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "SPAWN SupportTicket { subject: \"Login broken\", priority: \"high\" }"}'

# Transition (use the instance ID from the spawn response)
curl -s -X POST http://localhost:4200/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION SupportTicket \"01JEXAMPLE\" TO assigned"}'

# Query
curl -s -X POST http://localhost:4200/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "FIND SupportTicket WHERE STATE IS open LIMIT 10"}'
```

### 4. Use the REPL

```bash
# Interactive REPL with in-memory storage
smql repl

# REPL with RocksDB
smql repl --storage ./data.db
```

```
smql> DEFINE MACHINE Counter ( STATES { idle, running, done } INITIAL STATE idle TERMINAL STATES { done } TRANSITIONS { idle -> running {} running -> done {} } )
Machine defined: Counter

smql> SPAWN Counter {}
Spawned Counter instance: 01JF...

smql> .machines
Counter

smql> .states Counter
idle, running, done
```

Meta-commands: `.help`, `.machines`, `.states <machine>`, `.transitions <machine>`

### 5. Run Script Files

SMQL scripts support `$N` references to refer to previously spawned instance IDs:

```smql
DEFINE MACHINE Task ( STATES { todo, done } INITIAL STATE todo TERMINAL STATES { done } TRANSITIONS { todo -> done {} } )
SPAWN Task { title: "First task" }
SPAWN Task { title: "Second task" }
TRANSITION Task $1 TO done
GET $2
```

```bash
smql run script.smql
```

### 6. Execute One-off Statements

```bash
smql exec "SPAWN Counter {}"
smql exec "FIND Counter WHERE STATE IS idle" --storage ./data.db
```

## CLI Reference

```
smql serve    [--bind 127.0.0.1:4200] [--storage memory]
smql repl     [--storage memory]
smql exec     <statement> [--storage memory]
smql run      <file.smql> [--storage memory]
smql codegen  --input <path>... [--output src/generated] [--lang rust]
```

The `--storage` flag accepts either `memory` (default) or a filesystem path, which enables RocksDB persistent storage at that location.

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

### JWT Auth

When built with the `auth` feature (included in all prebuilt binaries), set the `SMQL_JWT_SECRET` environment variable to enable JWT authentication:

```bash
SMQL_JWT_SECRET=your-secret-key smql serve --storage ./data.db
```

All requests (except `/health` and `/metrics`) must include a `Authorization: Bearer <token>` header.

## SDK

The Rust SDK (`smql-sdk`) provides a typed client for the HTTP API:

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

### Code Generation

Generate typed Rust structs from `.smql` definitions:

```bash
smql codegen --input machines/ --output src/generated/
```

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
- **Storage backends**: In-memory (default) or RocksDB (persistent)
- **Observability**: Prometheus metrics, WebSocket event streaming, structured logging (tracing)
- **Webhooks**: HTTP POST actions with retry on 5xx/network errors
- **Auth**: JWT HS256 middleware
- **Timer persistence**: Write-through to storage, automatic restore on startup
- **SDK**: Ergonomic Rust client with builders, typed instances, WebSocket subscriptions
- **Codegen**: Generate typed Rust code from `.smql` definitions

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
