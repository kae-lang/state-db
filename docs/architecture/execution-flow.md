# Spawn & Transition Pipelines

This page walks through the two core execution paths in the SMQL engine: spawning a new instance and transitioning an existing one. Both are implemented in `smql-engine-core`.

## Spawn Pipeline

When the engine receives a `SPAWN Machine { field: value }` command, it executes the following 10-step pipeline:

### Step 1: Parse SPAWN command

The parser produces a `Command::Spawn` node containing the machine name and a map of initial field values. Data fields use `:` syntax, not `=`:

```
SPAWN SupportTicket { title: "Login broken", priority: "high" }
```

### Step 2: Look up machine in catalog

The engine calls `catalog.get("SupportTicket")` to retrieve the `Machine` definition from the `DashMap`-backed `MachineCatalog`. If the machine is not registered, this returns an error immediately.

### Step 3: Validate required fields

Every field marked as required in the machine definition must be present in the spawn data. Missing required fields produce a `ValidationError` with the field name.

### Step 4: Apply defaults

Fields with default values that are not present in the spawn data are filled in. Defaults are evaluated as expressions (they can reference other fields or call built-in functions like `now()`).

### Step 5: Validate constraints

Each field value is validated against its type and constraints:

- **Type check**: TEXT, INT, FLOAT, BOOL, MONEY, REF, DATETIME
- **Constraints**: min, max, range, pattern (regex), unique (checked against storage)

### Step 6: Create Instance

A new `Instance` is created with:
- A ULID (26 characters, not UUID) as the instance ID
- The machine's `initial` state as the current state
- Version 1
- `created_at` and `updated_at` timestamps
- The validated data map

### Step 7: Execute ON SPAWN hooks

The `HookExecutor` runs any `ON SPAWN` hooks defined in the machine. These hooks can EMIT events or perform side effects. Unlike BEFORE hooks, ON SPAWN hooks cannot reject the operation.

### Step 8: Store instance

The engine calls `storage.store_instance(&instance)`. For `MemoryStorage` this inserts into the instances DashMap and updates the state index. For RocksDB this writes to multiple column families atomically via `WriteBatch`.

### Step 9: Start timers

If the initial state has timeout transitions (e.g., `TIMEOUT 1h -> escalated`), the `TimerManager` schedules them. Each timer is keyed by `(instance_id, from_state)`.

### Step 10: Record trail entry

A trail entry is appended with sequence 0, an empty `from_state` (since this is the first entry), and the initial state as `to_state`. This is the spawn event in the audit trail.

## Transition Pipeline

When the engine receives a `TRANSITION MachineName "instance_id" TO state` command, it executes this 10-step pipeline:

### Step 1: Load instance from storage

```rust
let instance = storage.get_instance(&id).await?;
```

If the instance does not exist, the engine returns a `NotFound` error.

### Step 2: Optimistic version check

The caller may provide an expected version. The engine compares it against the stored version:

```rust
if expected_version != instance.version {
    return Err(SmqlError::Conflict { ... });
}
```

This prevents lost updates when two transitions race on the same instance.

### Step 3: Look up transition definition

The engine searches the machine definition for a matching transition. It tries two lookups in order:

1. **Exact match**: a transition from the current state to the target state
2. **ANY wildcard**: a transition from `ANY` to the target state (with optional `EXCEPT` list)

If neither matches, the transition is rejected.

### Step 4: Execute BEFORE EACH TRANSITION hooks

BEFORE hooks are synchronous and can reject the transition. If any BEFORE EACH TRANSITION hook returns a rejection, the entire transition is aborted. This is the only hook type that blocks execution.

```rust
// Pseudo-code
for hook in before_hooks {
    if hook.execute(&context).is_reject() {
        return Err(SmqlError::HookRejected { ... });
    }
}
```

### Step 5: Evaluate all guards

Guards are boolean expressions that must all evaluate to true (AND semantics). The engine calls `eval_expr()` for each guard with an `EvalContext` populated from the instance data and actor.

```rust
for guard in &transition.guards {
    let result = eval_expr(guard, &context)?;
    if result != Value::Bool(true) {
        return Err(SmqlError::GuardFailed { ... });
    }
}
```

**Important**: Timeout transitions bypass guards entirely. They are executed by the "System" actor and are unconditional.

### Step 6: Execute mutations

If the transition has a `MUTATE` clause, the engine evaluates each mutation expression. The `WITH` data from the command is also merged into the instance data:

```
TRANSITION SupportTicket "abc123" TO resolved WITH { resolution: "Fixed in v2.1" }
```

The `__spawn` function call is detected at this stage before `eval_expr()` runs, because spawn is an async operation that the synchronous evaluator cannot handle.

### Step 7: Update state, version, timestamps

```rust
instance.state = target_state;
instance.version += 1;
instance.updated_at = now();
```

### Step 8: Cancel old timers, start new timers

When an instance leaves a state, all timers for that state are cancelled via `timer_manager.cancel(instance_id, old_state)`. If the new state has timeout transitions, new timers are scheduled.

### Step 9: Store updated instance + trail entry

The engine calls `storage.update_instance(&instance, expected_version)` and `storage.append_trail(&id, trail_entry)`. The trail entry records from_state, to_state, actor, timestamp, and sequence number.

### Step 10: Fire async hooks and actions

The remaining hooks fire asynchronously and do not block the response:

- **ON EXIT** hooks for the old state
- **ON ENTER** hooks for the new state
- **AFTER EACH TRANSITION** hooks
- Any actions defined on the transition

These are all fire-and-forget via `tokio::spawn`.

## Multi-Hop Transitions (THROUGH)

A transition can pass through intermediate states using `THROUGH`:

```
open -> THROUGH review -> approved
```

This triggers a recursive transition. The engine transitions to the intermediate state first, then immediately transitions to the final state. Since Rust async functions cannot be recursive without boxing, the engine uses `Box::pin`:

```rust
fn do_transition<'a>(
    &'a self,
    instance_id: &'a InstanceId,
    target: &'a str,
    through: Option<&'a [String]>,
    actor: &'a Value,
) -> Pin<Box<dyn Future<Output = Result<Instance>> + Send + 'a>> {
    Box::pin(async move {
        // Transition to intermediate state
        if let Some((next, rest)) = through.and_then(|t| t.split_first()) {
            self.do_transition(instance_id, next, Some(rest), actor).await?;
        }
        // Then transition to final state
        self.execute_transition(instance_id, target, actor).await
    })
}
```

The `Box::pin` is necessary because async fn return types are opaque and cannot reference themselves. Wrapping the future in a `Pin<Box<...>>` with explicit lifetime annotations breaks the recursion at the type level.

## Timeout Transitions

Timeout transitions are special:

- They are scheduled by the `TimerManager` with a duration
- When the timer fires, the engine performs a transition with actor = `"System"`
- **Guards are not evaluated** — timeout transitions are unconditional
- If the instance has already left the state, the timer was cancelled in step 8 and never fires
- The engine polls the `TimerManager` periodically via `tokio::time::interval`

```
TIMEOUT 24h -> escalated
```

This creates a timer entry. If the instance is still in the source state after 24 hours, the engine transitions it to `escalated` without checking any guards.
