# Getting Started

The SMQL server exposes the engine as an HTTP/JSON API built on [axum](https://github.com/tokio-rs/axum). It supports real-time event streaming over WebSocket, Prometheus metrics, and pluggable storage backends.

## Starting the Server

Use the `serve` subcommand to start the HTTP server:

```bash
smql serve
```

By default the server binds to `127.0.0.1:4200`.

### Custom Bind Address

Pass `--bind` / `-b` to listen on a different address or port:

```bash
# Listen on all interfaces, port 8080
smql serve --bind 0.0.0.0:8080
```

### Choosing a Storage Backend

The `--storage` / `-s` flag selects the persistence layer:

```bash
# In-memory (default) -- data is lost when the server stops
smql serve --storage memory

# RocksDB -- persistent storage at the given path
smql serve --storage ./data
```

RocksDB requires the binary to be compiled with the `rocksdb` feature flag. See [Storage Backends](./storage-backends) for details.

## Environment Variables

### Logging

The server emits structured JSON logs via [tracing](https://docs.rs/tracing). Control the log level with the `RUST_LOG` environment variable:

```bash
# Default level is info
RUST_LOG=info smql serve

# Enable debug logging for the engine
RUST_LOG=smql_engine_core=debug smql serve

# Verbose trace logging for everything
RUST_LOG=trace smql serve

# Combine multiple directives
RUST_LOG=info,smql_server=debug,smql_engine_core=trace smql serve
```

When `RUST_LOG` is not set, the server defaults to `info`.

## Verifying the Server

After starting, confirm the server is running with a health check:

```bash
curl http://127.0.0.1:4200/health
```

Expected response:

```json
{"status": "ok"}
```

## Quick Walkthrough

Define a machine, spawn an instance, and query it -- all through the HTTP API:

```bash
# 1. Define a machine
curl -X POST http://127.0.0.1:4200/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "DEFINE MACHINE Task ( STATES { todo, in_progress, done } INITIAL STATE todo TERMINAL STATES { done } TRANSITIONS { todo -> in_progress {} in_progress -> done {} } )"}'

# 2. Spawn an instance
curl -X POST http://127.0.0.1:4200/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "SPAWN Task { title: \"Write docs\" }"}'

# 3. List machines
curl http://127.0.0.1:4200/machines

# 4. Check Prometheus metrics
curl http://127.0.0.1:4200/metrics
```

## What's Next

- [HTTP API Reference](./http-api) -- complete endpoint documentation
- [Request & Response Formats](./request-response) -- JSON examples for every command and query
- [WebSocket Events](./websocket) -- real-time event streaming
- [Storage Backends](./storage-backends) -- memory vs RocksDB
- [Observability](./observability) -- metrics, logging, and monitoring
