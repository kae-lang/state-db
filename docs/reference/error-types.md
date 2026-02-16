# Error Types

SMQL uses structured errors with context to help diagnose problems. All errors derive from the `SmqlError` enum.

## Error Variants

### ParseError

Returned when the SMQL input cannot be parsed.

| Field | Type | Description |
|-------|------|-------------|
| `message` | String | Human-readable error description |
| `span` | Option | Source location (offset, length) |
| `hint` | Option | Suggested fix |

```
Parse error: Expected 'TO' after instance ID
```

**HTTP Status**: 400 Bad Request

### ValidationError

Returned when data validation fails (constraints, types, schema).

| Field | Type | Description |
|-------|------|-------------|
| `message` | String | What failed |
| `field` | Option | Which field caused the error |
| `hint` | Option | Suggested fix |

```
Validation error: Field 'subject' exceeds MAX(200)
```

**HTTP Status**: 400 Bad Request

### TransitionDenied

Returned when a transition cannot proceed (guard failure, invalid state change).

Contains a `TransitionDeniedError` with:

| Field | Type | Description |
|-------|------|-------------|
| `instance_id` | String | The instance that was being transitioned |
| `from_state` | String | Current state |
| `to_state` | String | Requested target state |
| `guard_failures` | Vec | List of guard failures with details |
| `hint` | Option | Suggested fix |

Each `GuardFailure` contains:

| Field | Type | Description |
|-------|------|-------------|
| `guard_expr` | String | The guard expression that failed |
| `actual_value` | Option | What the expression evaluated to |
| `expected` | Option | What was expected |
| `hint` | Option | Contextual hint |

```
Transition denied: Transition open -> triaged denied for instance 01J5...:
  - Guard 'assignee IS SET' failed (got: null) — Set the assignee field before triaging
```

**HTTP Status**: 409 Conflict

### SpawnRejected

Returned when a SPAWN fails validation.

| Field | Type | Description |
|-------|------|-------------|
| `message` | String | Why the spawn was rejected |
| `field` | Option | Which field caused the issue |
| `hint` | Option | Suggested fix |

```
Spawn rejected: Required field 'customer_id' not provided
```

**HTTP Status**: 400 Bad Request

### GuardFailed

Returned when a specific guard expression fails.

| Field | Type | Description |
|-------|------|-------------|
| `message` | String | Description |
| `guard_expr` | String | The expression |
| `actual_value` | Option | Evaluated result |
| `hint` | Option | Suggested fix |

### QueryError

Returned when a query cannot be executed.

| Field | Type | Description |
|-------|------|-------------|
| `message` | String | Description |
| `hint` | Option | Suggested fix |

```
Query error: Machine 'Unknown' not found
```

**HTTP Status**: 500 Internal Server Error (or 400/404 depending on cause)

### StorageError

Returned when the storage backend encounters an error.

| Field | Type | Description |
|-------|------|-------------|
| `message` | String | Description |
| `retryable` | bool | Whether the operation can be retried |

**HTTP Status**: 500 Internal Server Error

### NotFound

Returned when a requested entity does not exist.

| Field | Type | Description |
|-------|------|-------------|
| `entity_type` | String | Type of entity (e.g., "Machine", "Instance") |
| `id` | String | The requested ID |

```
Not found: Instance '01J5...'
```

**HTTP Status**: 404 Not Found

### Conflict

Returned for optimistic locking failures (version mismatch).

| Field | Type | Description |
|-------|------|-------------|
| `message` | String | Description |
| `hint` | Option | Suggested fix |

```
Conflict: Version mismatch — expected 3, found 4
```

**HTTP Status**: 409 Conflict

### TimeoutError

Returned when a timeout operation fails.

| Field | Type | Description |
|-------|------|-------------|
| `message` | String | Description |
| `instance_id` | Option | Affected instance |
| `state` | Option | State where the timeout was set |

### Internal

Catch-all for unexpected errors.

| Field | Type | Description |
|-------|------|-------------|
| `message` | String | Description |

**HTTP Status**: 500 Internal Server Error

## HTTP Status Code Mapping

| Error Type | HTTP Status |
|-----------|-------------|
| ParseError | 400 |
| ValidationError | 400 |
| SpawnRejected | 400 |
| TransitionDenied | 409 |
| Conflict | 409 |
| NotFound | 404 |
| StorageError | 500 |
| Internal | 500 |
