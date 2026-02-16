# smql serve

Start the SMQL HTTP/JSON API server.

## Usage

```bash
smql serve [OPTIONS]
```

## Options

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--bind` | `-b` | `127.0.0.1:4200` | Address and port to bind the server to |
| `--storage` | `-s` | `memory` | Storage backend -- `"memory"` or a filesystem path for RocksDB |

## Logging

The server enables structured JSON logging via `tracing_subscriber`. Control the log level with the `RUST_LOG` environment variable. The default level is `info`.

## Examples

Start with default settings (localhost:4200, in-memory storage):

```bash
smql serve
```

Bind to all interfaces on port 8080:

```bash
smql serve --bind 0.0.0.0:8080
```

Use RocksDB-backed persistent storage:

```bash
smql serve --bind 127.0.0.1:4200 --storage ./data
```

Enable debug-level logging:

```bash
RUST_LOG=debug smql serve
```

Combine options for a production-style setup:

```bash
RUST_LOG=info smql serve --bind 0.0.0.0:4200 --storage /var/lib/smql/data
```

## API

Once running, the server exposes a REST API. Send SMQL statements via POST:

```bash
curl -X POST http://127.0.0.1:4200/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "DEFINE MACHINE Task ( STATES { todo, done } INITIAL STATE todo TERMINAL STATES { done } TRANSITIONS { todo -> done {} } )"}'
```
