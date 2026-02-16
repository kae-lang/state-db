# Configuration Reference

This page documents all configuration options for the SMQL server, CLI, and build system.

## CLI Flags

### smql serve

Start the HTTP server.

| Flag | Default | Description |
|------|---------|-------------|
| `--bind` | `127.0.0.1:4200` | Address and port to bind the server |
| `--storage` | `memory` | Storage backend: `memory` or a filesystem path for RocksDB |

```bash
# In-memory (development)
smql serve --bind 127.0.0.1:4200

# RocksDB (production)
smql serve --bind 0.0.0.0:4200 --storage ./data

# Custom port
smql serve --bind 127.0.0.1:8080
```

### smql repl

Start the interactive REPL.

| Flag | Default | Description |
|------|---------|-------------|
| `--storage` | `memory` | Storage backend: `memory` or a filesystem path |

```bash
smql repl
smql repl --storage ./data
```

REPL meta-commands:

| Command | Description |
|---------|-------------|
| `.help` | Show available commands |
| `.machines` | List registered machines |
| `.states <machine>` | Show states for a machine |
| `.transitions <machine>` | Show transitions for a machine |
| `.quit` | Exit the REPL |

### smql exec

Execute a single SMQL statement.

| Flag | Default | Description |
|------|---------|-------------|
| `--storage` | `memory` | Storage backend |

```bash
smql exec "SPAWN Counter {}" --storage memory
smql exec "FIND SupportTicket WHERE STATE IS open" --storage ./data
```

### smql run

Execute a script file containing multiple SMQL statements.

| Flag | Default | Description |
|------|---------|-------------|
| `--storage` | `memory` | Storage backend |

```bash
smql run setup.smql --storage memory
smql run migrations.smql --storage ./data
```

### smql codegen

Generate typed code from SMQL machine definitions.

| Flag | Default | Description |
|------|---------|-------------|
| `--input` | required | Input file or directory containing `.smql` files |
| `--output` | required | Output file or directory for generated code |
| `--lang` | `rust` | Target language |

```bash
# Single file
smql codegen --input machines/order.smql --output src/generated/order.rs

# Directory (generates one file per machine)
smql codegen --input machines/ --output src/generated/ --lang rust
```

## Storage Backends

### Memory

The default storage backend. All data is stored in memory and lost on restart.

```bash
smql serve --storage memory
```

Properties:
- No setup required
- Fastest performance (DashMap-based concurrent hash map)
- Data lost on process exit
- Best for development, testing, and prototyping

### RocksDB

Persistent storage using RocksDB. Data survives restarts.

```bash
smql serve --storage ./data
```

Properties:
- Data directory created automatically if it doesn't exist
- Uses 6 column families: instances, state_index, machine_index, trails, parent_index, id_index
- WriteBatch for atomic multi-write operations
- Requires building with the `rocksdb` feature flag

::: warning
The RocksDB storage directory should be on a local filesystem (not NFS or network-mounted). RocksDB uses memory-mapped files and file locks that may not work correctly on network storage.
:::

## Cargo Features

SMQL uses feature flags to control optional dependencies.

### rocksdb

Enables the RocksDB storage backend.

```bash
# Build with RocksDB support
cargo build --release --features rocksdb
```

Affects these crates:
- `smql-storage` — includes the RocksDB `Storage` implementation
- `smql-server` — accepts filesystem paths in `--storage`
- `smql-cli` — accepts filesystem paths in `--storage`

Without this feature, `--storage` only accepts `memory`.

## Server Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check (returns 200 OK) |
| `POST` | `/execute` | Execute raw SMQL statement |
| `GET` | `/machines` | List all registered machines |
| `GET` | `/machines/:name` | Get machine definition by name |
| `GET` | `/instances/:id` | Get instance by ID |
| `GET` | `/metrics` | Prometheus metrics (text format) |
| `GET` | `/subscribe` | WebSocket for event streaming |

### Execute Endpoint

The primary endpoint for all SMQL operations.

**Request:**
```json
{ "smql": "SMQL_STATEMENT_HERE" }
```

**Response (success):**
```json
{ "success": true, "result": { ... } }
```

**Response (error):**
```json
{ "success": false, "error": "Error message" }
```

### WebSocket Endpoint

Connect to `/subscribe` for real-time event streaming.

**Query parameters:**

| Parameter | Required | Description |
|-----------|----------|-------------|
| `machine` | No | Filter events by machine name |
| `event` | No | Filter events by event name |

```
ws://localhost:4200/subscribe
ws://localhost:4200/subscribe?machine=SupportTicket
ws://localhost:4200/subscribe?machine=SupportTicket&event=ticket.assigned
```

**Event format:**
```json
{
  "event": "ticket.assigned",
  "machine": "SupportTicket",
  "instance_id": "01JM...",
  "actor": "agent_1",
  "data": { ... }
}
```

## Prometheus Metrics

Metrics are exposed at `/metrics` in the standard Prometheus text format.

| Metric | Type | Labels |
|--------|------|--------|
| `smql_instances_total` | Gauge | `machine`, `state` |
| `smql_transitions_total` | Counter | `machine`, `from`, `to` |
| `smql_transition_duration_seconds` | Histogram | `machine` |
| `smql_spawns_total` | Counter | `machine` |
| `smql_guard_failures_total` | Counter | `machine` |
| `smql_timeout_fires_total` | Counter | `machine`, `state` |
| `smql_query_duration_seconds` | Histogram | `query_type` |
| `smql_state_dwell_seconds` | Histogram | `machine`, `state` |

::: info
Metrics only appear in the output after at least one observation. If you see an empty `/metrics` response, it means no operations have been performed yet.
:::

## Structured Logging

SMQL uses the `tracing` crate for structured logging. Set the log level with the `RUST_LOG` environment variable:

```bash
# Default (info)
smql serve --bind 127.0.0.1:4200

# Debug logging
RUST_LOG=debug smql serve --bind 127.0.0.1:4200

# Trace logging for the engine crate
RUST_LOG=smql_engine=trace smql serve --bind 127.0.0.1:4200

# JSON format logging
RUST_LOG=info smql serve --bind 127.0.0.1:4200
```

Logged events include spans for:
- `spawn` — machine field
- `transition_inner` — machine field
- `timeout_transition` — machine field
- `execute_query` — query_type field
