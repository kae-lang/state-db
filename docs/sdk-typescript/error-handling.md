# Error Handling

All async SDK methods throw typed errors on failure. The error hierarchy maps directly to HTTP status codes returned by the SMQL server.

## Error Hierarchy

```typescript
import {
  SmqlError,          // Base class
  BadRequestError,    // 400
  UnauthorizedError,  // 401
  NotFoundError,      // 404
  TransitionDeniedError, // 409
  NetworkError,       // fetch failure
  TimeoutError,       // AbortController timeout
  SubscriptionError,  // WebSocket errors
  SmqlErrorCode,      // Enum of error codes
} from "smql-sdk";
```

All errors extend `SmqlError`, which has:

| Property | Type | Description |
|----------|------|-------------|
| `message` | `string` | Human-readable error description |
| `code` | `SmqlErrorCode` | Enum code for programmatic handling |
| `statusCode` | `number?` | HTTP status code, if applicable |

## Error Types

### `BadRequestError` (400)

The SMQL statement has a syntax error, references an unknown machine, or is otherwise invalid.

```typescript
try {
  await client.execute("INVALID SMQL");
} catch (err) {
  if (err instanceof BadRequestError) {
    console.log("Bad request:", err.message);
  }
}
```

### `UnauthorizedError` (401)

The request requires authentication or the provided token is invalid.

```typescript
try {
  await client.listMachines();
} catch (err) {
  if (err instanceof UnauthorizedError) {
    console.log("Need to authenticate");
  }
}
```

### `NotFoundError` (404)

The requested instance or machine does not exist.

```typescript
try {
  await client.getInstance("nonexistent-id");
} catch (err) {
  if (err instanceof NotFoundError) {
    console.log("Not found:", err.message);
  }
}
```

### `TransitionDeniedError` (409)

A transition was rejected because a guard condition was not met, the transition is not allowed from the current state, or a rule/policy rejected it.

```typescript
try {
  await client.transition("Order", id, "shipped").execute();
} catch (err) {
  if (err instanceof TransitionDeniedError) {
    console.log("Cannot transition:", err.message);
  }
}
```

Use `tryTransition` to avoid catching this error:

```typescript
// These are equivalent:
const result = await client.tryTransition("Order", id, "shipped").execute();
if (!result.transitioned) {
  console.log("Transition denied");
}

// vs.
try {
  const result = await client.transition("Order", id, "shipped").execute();
} catch (err) {
  if (err instanceof TransitionDeniedError) {
    console.log("Transition denied");
  } else {
    throw err;
  }
}
```

### `NetworkError`

The `fetch` call itself failed -- network unreachable, DNS resolution failure, connection refused.

```typescript
try {
  await client.health();
} catch (err) {
  if (err instanceof NetworkError) {
    console.log("Cannot reach server:", err.message);
  }
}
```

### `TimeoutError`

The request exceeded the configured timeout (`SmqlClientConfig.timeout`).

```typescript
try {
  await client.find("HugeTable").execute();
} catch (err) {
  if (err instanceof TimeoutError) {
    console.log("Request timed out");
  }
}
```

### `SubscriptionError`

A WebSocket connection error -- failed to connect, connection dropped, or unparseable message.

```typescript
try {
  const sub = client.subscribe();
  await sub.connect();
} catch (err) {
  if (err instanceof SubscriptionError) {
    console.log("WebSocket error:", err.message);
  }
}
```

## SmqlErrorCode Enum

For `switch`-based handling:

```typescript
import { SmqlError, SmqlErrorCode } from "smql-sdk";

try {
  await client.spawn("Order").set({ amount: -1 }).execute();
} catch (err) {
  if (err instanceof SmqlError) {
    switch (err.code) {
      case SmqlErrorCode.BadRequest:
        console.log("Invalid request");
        break;
      case SmqlErrorCode.NotFound:
        console.log("Not found");
        break;
      case SmqlErrorCode.TransitionDenied:
        console.log("Transition denied");
        break;
      case SmqlErrorCode.Unauthorized:
        console.log("Unauthorized");
        break;
      case SmqlErrorCode.Network:
      case SmqlErrorCode.Timeout:
        console.log("Connectivity issue");
        break;
      default:
        console.log("Other error:", err.message);
    }
  }
}
```

## Retry Pattern

```typescript
import { SmqlError, SmqlErrorCode, NetworkError, TimeoutError } from "smql-sdk";

function isRetryable(err: unknown): boolean {
  return err instanceof NetworkError || err instanceof TimeoutError;
}

async function withRetry<T>(fn: () => Promise<T>, maxRetries = 3): Promise<T> {
  let attempts = 0;
  while (true) {
    try {
      return await fn();
    } catch (err) {
      if (isRetryable(err) && attempts < maxRetries) {
        attempts++;
        const delay = 100 * Math.pow(2, attempts);
        console.log(`Attempt ${attempts} failed, retrying in ${delay}ms...`);
        await new Promise((r) => setTimeout(r, delay));
      } else {
        throw err;
      }
    }
  }
}

// Usage
const instance = await withRetry(() =>
  client.spawn("Order").set({ item: "Widget" }).execute()
);
```
