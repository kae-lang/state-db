# Request & Response Formats

All commands and queries are sent as `POST /execute` with a JSON body containing the SMQL statement. This page shows the request and response for every supported operation.

## Commands

### DEFINE MACHINE

Registers a new machine definition in the catalog.

**Request:**

```bash
curl -X POST http://127.0.0.1:4200/execute \
  -H "Content-Type: application/json" \
  -d @- <<'EOF'
{
  "smql": "DEFINE MACHINE SupportTicket ( STATES { open, assigned, resolved, closed } INITIAL STATE open TERMINAL STATES { closed } TRANSITIONS { open -> assigned { GUARD : ACTOR.role == \"support\" } assigned -> resolved {} resolved -> closed {} } )"
}
EOF
```

**Response** `201 Created`:

```json
{
  "success": true,
  "result": {
    "action": "machine_defined"
  }
}
```

If the definition produces warnings (e.g., unreachable states), they appear in the `warnings` array:

```json
{
  "success": true,
  "result": {
    "action": "machine_defined"
  },
  "warnings": ["State 'orphan' is unreachable from initial state"]
}
```

**Error** `400 Bad Request` -- duplicate machine name or invalid definition:

```json
{
  "success": false,
  "error": "Machine 'SupportTicket' already exists"
}
```

### SPAWN

Creates a new instance of a machine in its initial state.

**Request:**

```json
{
  "smql": "SPAWN SupportTicket { title: \"Login page broken\", priority: 1 }"
}
```

**Response** `201 Created`:

```json
{
  "success": true,
  "result": {
    "id": "01HXYZ1234567890ABCDEFGHIJ",
    "machine": "SupportTicket",
    "state": "open",
    "data": {
      "title": "Login page broken",
      "priority": 1
    },
    "created_at": "2026-02-16T10:00:00+00:00",
    "updated_at": "2026-02-16T10:00:00+00:00",
    "state_entered_at": "2026-02-16T10:00:00+00:00",
    "trail_length": 1,
    "version": 1
  }
}
```

::: tip
SPAWN always requires braces even with no data: `SPAWN MachineName {}`
:::

### TRANSITION

Moves an instance from its current state to a target state.

**Request:**

```json
{
  "smql": "TRANSITION SupportTicket \"01HXYZ1234567890ABCDEFGHIJ\" TO assigned AS \"agent-7\" WITH { assignee: { id: \"agent-7\", role: \"support\" } }"
}
```

**Response** `200 OK`:

```json
{
  "success": true,
  "result": {
    "from_state": "open",
    "to_state": "assigned",
    "instance": {
      "id": "01HXYZ1234567890ABCDEFGHIJ",
      "machine": "SupportTicket",
      "state": "assigned",
      "data": {
        "title": "Login page broken",
        "priority": 1,
        "assignee": {"id": "agent-7", "role": "support"}
      },
      "created_at": "2026-02-16T10:00:00+00:00",
      "updated_at": "2026-02-16T10:05:00+00:00",
      "state_entered_at": "2026-02-16T10:05:00+00:00",
      "trail_length": 2,
      "version": 2
    }
  }
}
```

**Error** `409 Conflict` -- guard rejects the transition:

```json
{
  "success": false,
  "error": "Transition denied: guard condition not met for open -> assigned"
}
```

**Error** `404 Not Found` -- instance does not exist:

```json
{
  "success": false,
  "error": "Instance '01HXYZ...' not found"
}
```

### TRY TRANSITION

Attempts a transition but returns success even if the guard fails. Never returns `409`.

**Request:**

```json
{
  "smql": "TRY TRANSITION SupportTicket \"01HXYZ1234567890ABCDEFGHIJ\" TO assigned"
}
```

**Response** `200 OK` -- guard passed, transition happened:

```json
{
  "success": true,
  "result": {
    "transitioned": true,
    "from_state": "open",
    "to_state": "assigned",
    "instance": {
      "id": "01HXYZ1234567890ABCDEFGHIJ",
      "machine": "SupportTicket",
      "state": "assigned",
      "data": { ... },
      "created_at": "...",
      "updated_at": "...",
      "state_entered_at": "...",
      "trail_length": 2,
      "version": 2
    }
  }
}
```

**Response** `200 OK` -- guard failed, no transition:

```json
{
  "success": true,
  "result": {
    "transitioned": false
  }
}
```

### ALTER MACHINE

Modifies an existing machine definition. Supports adding/removing states, adding/removing transitions, and instance migration.

**Request:**

```json
{
  "smql": "ALTER MACHINE SupportTicket ADD STATE escalated"
}
```

**Response** `200 OK`:

```json
{
  "success": true,
  "result": {
    "action": "machine_altered",
    "machine": "SupportTicket",
    "new_version": 2,
    "operations_applied": 1,
    "instances_migrated": 0
  }
}
```

When the alter produces warnings:

```json
{
  "success": true,
  "result": {
    "action": "machine_altered",
    "machine": "SupportTicket",
    "new_version": 3,
    "operations_applied": 2,
    "instances_migrated": 5
  },
  "warnings": ["Migrated 5 instances from removed state 'legacy' to 'open'"]
}
```

## Queries

All queries are also sent via `POST /execute`.

### GET

Retrieve a single instance by machine name and instance ID.

**Request:**

```json
{
  "smql": "GET SupportTicket \"01HXYZ1234567890ABCDEFGHIJ\""
}
```

**Response** `200 OK`:

```json
{
  "success": true,
  "result": {
    "id": "01HXYZ1234567890ABCDEFGHIJ",
    "machine": "SupportTicket",
    "state": "assigned",
    "data": {
      "title": "Login page broken",
      "priority": 1
    },
    "created_at": "2026-02-16T10:00:00+00:00",
    "updated_at": "2026-02-16T10:05:00+00:00",
    "state_entered_at": "2026-02-16T10:05:00+00:00",
    "trail_length": 2,
    "version": 2
  }
}
```

### FIND

Search for instances by machine, state, and data filters.

**Request:**

```json
{
  "smql": "FIND SupportTicket WHERE STATE IS assigned AND priority == 1"
}
```

**Response** `200 OK`:

```json
{
  "success": true,
  "result": {
    "count": 2,
    "instances": [
      {
        "id": "01HXYZ...",
        "machine": "SupportTicket",
        "state": "assigned",
        "data": { "title": "Login broken", "priority": 1 },
        "created_at": "...",
        "updated_at": "...",
        "state_entered_at": "...",
        "trail_length": 2,
        "version": 2
      },
      {
        "id": "01HABC...",
        "machine": "SupportTicket",
        "state": "assigned",
        "data": { "title": "Signup error", "priority": 1 },
        "created_at": "...",
        "updated_at": "...",
        "state_entered_at": "...",
        "trail_length": 3,
        "version": 3
      }
    ],
    "next_cursor": "01HABC..."
  }
}
```

The `next_cursor` field contains the ULID of the last instance in the result set. Pass it to `AFTER` in the next query for cursor-based pagination:

```json
{
  "smql": "FIND SupportTicket WHERE STATE IS assigned LIMIT 20 AFTER \"01HABC...\""
}
```

A FIND with no matches returns an empty list (no `next_cursor`):

```json
{
  "success": true,
  "result": {
    "count": 0,
    "instances": []
  }
}
```

### TRAIL

Retrieve the full audit trail (state transition history) for an instance.

**Request:**

```json
{
  "smql": "TRAIL OF \"01HXYZ1234567890ABCDEFGHIJ\""
}
```

**Response** `200 OK`:

```json
{
  "success": true,
  "result": {
    "count": 3,
    "entries": [
      {
        "sequence": 0,
        "from_state": "",
        "to_state": "open",
        "actor": null,
        "memo": null,
        "timestamp": "2026-02-16T10:00:00+00:00"
      },
      {
        "sequence": 1,
        "from_state": "open",
        "to_state": "assigned",
        "actor": "agent-7",
        "memo": null,
        "timestamp": "2026-02-16T10:05:00+00:00"
      },
      {
        "sequence": 2,
        "from_state": "assigned",
        "to_state": "resolved",
        "actor": "agent-7",
        "memo": "Fixed the CSS",
        "timestamp": "2026-02-16T11:30:00+00:00"
      }
    ]
  }
}
```

Sequence `0` is always the spawn event, with an empty `from_state`.

### AGGREGATE

Run aggregate queries with grouping and measures.

**Request:**

```json
{
  "smql": "AGGREGATE SupportTicket MEASURE COUNT() GROUP BY STATE"
}
```

**Response** `200 OK`:

```json
{
  "success": true,
  "result": {
    "rows": [
      {
        "group": { "state": "open" },
        "measures": { "COUNT": 12 }
      },
      {
        "group": { "state": "assigned" },
        "measures": { "COUNT": 7 }
      },
      {
        "group": { "state": "closed" },
        "measures": { "COUNT": 45 }
      }
    ]
  }
}
```

Measure keys use the function name in uppercase (`COUNT`, `SUM`, `AVG`, etc.) unless an alias is provided via `AS`:

```json
{
  "smql": "AGGREGATE SupportTicket MEASURE COUNT() AS total, SUM(points) AS total_points"
}
```

```json
{
  "success": true,
  "result": {
    "rows": [
      {
        "group": {},
        "measures": { "total": 64, "total_points": 320 }
      }
    ]
  }
}
```

### PATHS

Analyze the state transition paths taken by instances.

**Request:**

```json
{
  "smql": "PATHS FROM SupportTicket"
}
```

**Response** `200 OK`:

```json
{
  "success": true,
  "result": {
    "paths": [
      {
        "path": ["", "open", "assigned", "resolved", "closed"],
        "count": 38
      },
      {
        "path": ["", "open", "assigned", "closed"],
        "count": 5
      },
      {
        "path": ["", "open", "closed"],
        "count": 2
      }
    ]
  }
}
```

Paths start with an empty string `""` representing the spawn event (the initial `from_state` before entering the first state). Results are sorted by `count` descending.

### FUNNEL

Measure conversion rates through a sequence of states.

**Request:**

```json
{
  "smql": "FUNNEL SupportTicket THROUGH [open, assigned, resolved, closed]"
}
```

**Response** `200 OK`:

```json
{
  "success": true,
  "result": {
    "stages": [
      {
        "state": "open",
        "count": 100,
        "conversion_rate": 1.0
      },
      {
        "state": "assigned",
        "count": 85,
        "conversion_rate": 0.85
      },
      {
        "state": "resolved",
        "count": 72,
        "conversion_rate": 0.72
      },
      {
        "state": "closed",
        "count": 70,
        "conversion_rate": 0.7
      }
    ]
  }
}
```

The `conversion_rate` is relative to the **total number of matching instances**, not the previous stage. For example, 72 out of 100 = 0.72. To calculate stage-to-stage drop-off, divide adjacent counts.

### COMPARE PATHS

Analyze transition paths segmented by a data field.

**Request:**

```json
{
  "smql": "COMPARE PATHS SupportTicket SEGMENT BY priority"
}
```

**Response** `200 OK`:

```json
{
  "success": true,
  "result": {
    "segment_by": "priority",
    "segments": [
      {
        "segment_value": "high",
        "paths": [
          {
            "path": ["", "open", "triaged", "in_progress", "resolved", "closed"],
            "count": 25
          },
          {
            "path": ["", "open", "triaged", "in_progress", "escalated", "resolved", "closed"],
            "count": 8
          }
        ]
      },
      {
        "segment_value": "low",
        "paths": [
          {
            "path": ["", "open", "triaged", "resolved", "closed"],
            "count": 15
          }
        ]
      }
    ]
  }
}
```

### TRANSITION ALL

Transition all matching instances at once.

**Request:**

```json
{
  "smql": "TRANSITION ALL SupportTicket WHERE STATE IS resolved TO closed"
}
```

**Response** `200 OK`:

```json
{
  "success": true,
  "result": {
    "action": "batch_transition",
    "machine": "SupportTicket",
    "matched": 15,
    "transitioned": 12,
    "failed": 3,
    "failures": [
      {
        "instance_id": "01HXYZ...",
        "error": "Transition denied: guard condition not met for resolved -> closed"
      }
    ]
  }
}
```

## Error Envelope

All errors follow the same envelope format:

```json
{
  "success": false,
  "error": "Human-readable error message"
}
```

The HTTP status code indicates the error category:

| Status | Typical Cause |
|--------|---------------|
| `400` | Parse error, validation error, invalid data, spawn rejected |
| `404` | Machine or instance not found |
| `409` | Guard failure (transition denied), version conflict |
| `500` | Unexpected internal error |
