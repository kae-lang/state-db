# Observability

The SMQL server provides three observability channels: Prometheus metrics, structured JSON logging, and real-time WebSocket event streaming.

## Prometheus Metrics

All metrics are exposed at `GET /metrics` in Prometheus text exposition format (`text/plain; version=0.0.4`).

### Metric Reference

#### smql_spawns_total

**Type:** Counter

Total number of instances spawned, labeled by machine.

| Label | Description |
|-------|-------------|
| `machine` | Machine name |

```text
smql_spawns_total{machine="SupportTicket"} 42
smql_spawns_total{machine="Order"} 156
```

#### smql_instances_total

**Type:** Gauge

Current number of active instances, labeled by machine and state. Incremented on spawn and incoming transitions, decremented on outgoing transitions.

| Label | Description |
|-------|-------------|
| `machine` | Machine name |
| `state` | Current state name |

```text
smql_instances_total{machine="SupportTicket",state="open"} 12
smql_instances_total{machine="SupportTicket",state="assigned"} 7
smql_instances_total{machine="SupportTicket",state="closed"} 45
```

#### smql_transitions_total

**Type:** Counter

Total number of successful transitions, labeled by machine, source state, and target state.

| Label | Description |
|-------|-------------|
| `machine` | Machine name |
| `from` | Source state |
| `to` | Target state |

```text
smql_transitions_total{from="open",machine="SupportTicket",to="assigned"} 35
smql_transitions_total{from="assigned",machine="SupportTicket",to="resolved"} 28
```

#### smql_transition_duration_seconds

**Type:** Histogram

Duration of spawn and transition operations in seconds, labeled by machine. Includes guard evaluation, mutate execution, action execution, and storage writes.

| Label | Description |
|-------|-------------|
| `machine` | Machine name |

```text
smql_transition_duration_seconds_bucket{machine="SupportTicket",le="0.005"} 120
smql_transition_duration_seconds_bucket{machine="SupportTicket",le="0.01"} 135
...
smql_transition_duration_seconds_sum{machine="SupportTicket"} 0.892
smql_transition_duration_seconds_count{machine="SupportTicket"} 140
```

#### smql_guard_failures_total

**Type:** Counter

Total number of guard failures (transition denied), labeled by machine. Incremented for both `TRANSITION` (which returns 409) and `TRY TRANSITION` (which returns 200 with `transitioned: false`).

| Label | Description |
|-------|-------------|
| `machine` | Machine name |

```text
smql_guard_failures_total{machine="SupportTicket"} 3
```

#### smql_timeout_fires_total

**Type:** Counter

Total number of timeout transitions fired by the timer system. Tracked via an EventBus subscriber that watches for events named `TIMEOUT`.

| Label | Description |
|-------|-------------|
| `machine` | Machine name |
| `state` | State where the timeout fired (may be empty) |

```text
smql_timeout_fires_total{machine="Order",state=""} 5
```

#### smql_query_duration_seconds

**Type:** Histogram

Duration of query execution in seconds, labeled by query type.

| Label | Description |
|-------|-------------|
| `query_type` | One of: `GET`, `FIND`, `TRAIL`, `AGGREGATE`, `PATHS`, `FUNNEL`, `COMPARE_PATHS` |

```text
smql_query_duration_seconds_bucket{query_type="FIND",le="0.005"} 50
smql_query_duration_seconds_bucket{query_type="FIND",le="0.01"} 55
...
smql_query_duration_seconds_sum{query_type="FIND"} 0.234
smql_query_duration_seconds_count{query_type="FIND"} 60
```

### Scraping with Prometheus

Add a scrape target to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'smql'
    scrape_interval: 15s
    static_configs:
      - targets: ['127.0.0.1:4200']
```

### Grafana Dashboard Queries

Some useful PromQL queries for dashboards:

```txt
# Spawn rate per machine (per second)
rate(smql_spawns_total[5m])

# Transition rate by machine
rate(smql_transitions_total[5m])

# 99th percentile transition latency
histogram_quantile(0.99, rate(smql_transition_duration_seconds_bucket[5m]))

# Guard failure rate
rate(smql_guard_failures_total[5m])

# Current instances by state
smql_instances_total

# Timeout fire rate
rate(smql_timeout_fires_total[5m])

# Average query duration by type
rate(smql_query_duration_seconds_sum[5m]) / rate(smql_query_duration_seconds_count[5m])
```

::: tip
Prometheus metrics only appear in the `/metrics` output after at least one observation. A freshly started server with no activity returns only metric type and help comments.
:::

## Structured Logging

The server uses [tracing](https://docs.rs/tracing) with a JSON formatter. All log output goes to stdout.

### Log Format

Each line is a JSON object:

```json
{
  "timestamp": "2026-02-16T10:00:00.123456Z",
  "level": "INFO",
  "target": "smql_server::server",
  "message": "SMQL server listening on 127.0.0.1:4200"
}
```

### Configuring Log Level

Set the `RUST_LOG` environment variable before starting the server:

```bash
# Show info and above (default)
RUST_LOG=info smql serve

# Debug logging for the engine, info for everything else
RUST_LOG=info,smql_engine_core=debug smql serve

# Trace-level logging for all SMQL crates
RUST_LOG=info,smql_engine_core=trace,smql_server=trace,smql_storage=debug smql serve

# Suppress most output, only show warnings and errors
RUST_LOG=warn smql serve
```

When `RUST_LOG` is not set, the server defaults to `info`.

### Key Log Events

| Level | Source | Message |
|-------|--------|---------|
| `INFO` | `smql_server` | Server startup with bind address |
| `WARN` | `smql_server` | WebSocket subscriber lagged (missed events) |
| `WARN` | `smql_server` | Metrics EventBus subscriber lagged |
| `DEBUG` | `smql_engine_core` | Individual transition execution details |
| `TRACE` | `smql_engine_core` | Guard evaluation, mutate execution |

### Log Aggregation

Since logs are structured JSON, they work directly with log aggregation tools:

```bash
# Pipe to jq for pretty-printing during development
RUST_LOG=debug smql serve 2>&1 | jq .

# Ship to a file for log rotation
smql serve >> /var/log/smql/server.log 2>&1
```

## WebSocket Event Streaming

For real-time operational monitoring, connect to the WebSocket endpoint at `/subscribe`. This provides a live stream of all engine events (spawns, transitions, hook emissions, timeouts).

See [WebSocket Events](./websocket) for the full connection and message format documentation.

### Monitoring Use Cases

- **Live dashboards**: Stream events to a web UI showing real-time state changes.
- **Alerting**: Watch for specific event patterns (e.g., too many guard failures in a window).
- **Audit logging**: Forward all events to an external audit system.
- **Debugging**: Observe the exact sequence of events during development.

### Example: Event Counter

```bash
# Count events per second using websocat and jq
websocat ws://127.0.0.1:4200/subscribe | \
  jq -r '.event' | \
  uniq -c
```

## Architecture Notes

- Metrics are collected in the HTTP handler layer (`smql-server`), not in the engine. This keeps `smql-engine-core` free of monitoring dependencies.
- Timeout metrics are tracked by a background `tokio::spawn` task that subscribes to the EventBus and watches for events named `TIMEOUT`.
- The `SmqlMetrics` struct uses the `prometheus` crate's `Registry` for isolated metric collection (not the global default registry).
