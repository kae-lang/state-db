# HTTP API Reference

All endpoints are served from a single axum router. The default base URL is `http://127.0.0.1:4200`.

## Endpoint Summary

| Method | Path | Purpose | Response Content-Type |
|--------|------|---------|-----------------------|
| `GET` | `/health` | Health check | `application/json` |
| `POST` | `/execute` | Execute an SMQL statement | `application/json` |
| `GET` | `/machines` | List all registered machines | `application/json` |
| `GET` | `/machines/:name` | Get machine definition info | `application/json` |
| `GET` | `/instances/:id` | Get an instance by ID | `application/json` |
| `GET` | `/metrics` | Prometheus metrics | `text/plain` |
| `GET` | `/subscribe` | WebSocket upgrade | WebSocket |

## Health Check

```
GET /health
```

Returns a simple status object. Use this for load-balancer probes.

**Response** `200 OK`

```json
{
  "status": "ok"
}
```

## Execute SMQL

```
POST /execute
Content-Type: application/json
```

The primary endpoint. Accepts any single SMQL statement -- commands (`DEFINE`, `SPAWN`, `TRANSITION`, `TRY TRANSITION`, `ALTER MACHINE`) and queries (`GET`, `FIND`, `TRAIL`, `AGGREGATE`, `PATHS`, `FUNNEL`).

**Request body:**

```json
{
  "smql": "SPAWN Task { title: \"Buy milk\" }"
}
```

**Response envelope:**

```json
{
  "success": true,
  "result": { ... },
  "error": null,
  "warnings": null
}
```

Fields `result`, `error`, and `warnings` are omitted when `null`.

### Status Codes

| Code | Meaning | When |
|------|---------|------|
| `200` | OK | Successful queries, transitions, alter operations |
| `201` | Created | `DEFINE MACHINE` or `SPAWN` succeeded |
| `400` | Bad Request | Parse error, validation error, spawn rejected |
| `404` | Not Found | Machine or instance does not exist |
| `409` | Conflict | Transition denied (guard failure), version conflict |
| `500` | Internal Server Error | Unexpected engine error |
| `501` | Not Implemented | `BATCH TRANSITION` (not yet supported) |

See [Request & Response Formats](./request-response) for detailed examples of each command and query type.

## List Machines

```
GET /machines
```

Returns the names of all registered machine definitions.

**Response** `200 OK`

```json
{
  "machines": ["SupportTicket", "Order", "CIPipeline"]
}
```

Returns an empty array when no machines are defined.

## Get Machine

```
GET /machines/:name
```

Returns the definition summary for a single machine.

**Response** `200 OK`

```json
{
  "name": "SupportTicket",
  "states": ["open", "assigned", "resolved", "closed"],
  "initial_state": "open",
  "terminal_states": ["closed"],
  "version": 1
}
```

**Response** `404 Not Found`

```json
{
  "error": "Machine 'NonExistent' not found"
}
```

## Get Instance

```
GET /instances/:id
```

Returns the full state of an instance. The `:id` parameter must be a valid ULID (26 characters).

**Response** `200 OK`

```json
{
  "id": "01HXYZ1234567890ABCDEFGHIJ",
  "machine": "SupportTicket",
  "state": "assigned",
  "data": {
    "title": "Login broken",
    "priority": 1,
    "assignee": {"id": "agent-7", "role": "support"}
  },
  "created_at": "2026-02-16T10:00:00+00:00",
  "updated_at": "2026-02-16T10:05:00+00:00",
  "state_entered_at": "2026-02-16T10:05:00+00:00",
  "trail_length": 2,
  "version": 2
}
```

**Response** `400 Bad Request` -- invalid ID format:

```json
{
  "error": "Invalid instance ID"
}
```

**Response** `404 Not Found`:

```json
{
  "error": "Instance '01HXYZ1234567890ABCDEFGHIJ' not found"
}
```

## Prometheus Metrics

```
GET /metrics
```

Returns all collected metrics in Prometheus text exposition format. See [Observability](./observability) for the full list of metric names and labels.

**Response** `200 OK` with `Content-Type: text/plain; version=0.0.4; charset=utf-8`

```text
# HELP smql_spawns_total Total spawned instances by machine
# TYPE smql_spawns_total counter
smql_spawns_total{machine="SupportTicket"} 42
...
```

## WebSocket Subscribe

```
GET /subscribe
```

Upgrades the connection to a WebSocket for real-time event streaming. Supports optional query parameters for filtering.

| Parameter | Type | Description |
|-----------|------|-------------|
| `machine` | `string` | Only receive events for this machine |
| `event` | `string` | Only receive events with this name |

Example URLs:

```
ws://127.0.0.1:4200/subscribe
ws://127.0.0.1:4200/subscribe?machine=SupportTicket
ws://127.0.0.1:4200/subscribe?event=spawned
ws://127.0.0.1:4200/subscribe?machine=Order&event=payment_received
```

See [WebSocket Events](./websocket) for the message format and usage patterns.

## Value Serialization

SMQL values are serialized to JSON as follows:

| SMQL Type | JSON Representation |
|-----------|---------------------|
| `Text` | `"string"` |
| `Int` | `42` |
| `Float` | `3.14` |
| `Bool` | `true` / `false` |
| `Null` | `null` |
| `DateTime` | `"2026-02-16T10:00:00+00:00"` (RFC 3339) |
| `Date` | `"2026-02-16"` |
| `Duration` | `"1h"` / `"30m"` / `"1d"` |
| `Uuid` | `"550e8400-e29b-41d4-a716-446655440000"` |
| `List` | `[1, "two", true]` |
| `Set` | `[1, 2, 3]` (serialized as array) |
| `Map` | `{"key": "value"}` |
| `Ref` | `{"ref": "Order#01HX..."}` |
| `Money` | `{"amount": 9999, "currency": "USD"}` |
| `Blob` | `{"blob_size": 1024}` |
| `Json` | passed through as-is |
