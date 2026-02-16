# DashMap, Arc, Optimistic Locking

The SMQL engine is designed for concurrent access from multiple clients. This page explains the three mechanisms that make that work: `DashMap` for lock-free data structures, `Arc` for shared ownership, and optimistic locking for safe instance updates.

## DashMap

`DashMap` is a concurrent hash map that shards its entries across multiple internal locks. Two threads accessing different keys will almost never contend. This is used in two critical places:

### MachineCatalog

```rust
pub struct MachineCatalog {
    machines: DashMap<String, Machine>,
}
```

Machine definitions are read far more often than they are written (machines are registered at startup; every spawn and transition reads the definition). DashMap's sharded design makes reads essentially lock-free when there is no write contention on the same shard.

### MemoryStorage

```rust
pub struct MemoryStorage {
    instances: DashMap<InstanceId, Instance>,
    state_index: DashMap<(String, String), HashSet<InstanceId>>,
    trails: DashMap<InstanceId, Vec<TrailEntry>>,
    parent_index: DashMap<InstanceId, Vec<InstanceId>>,
}
```

Each DashMap can be read and written independently per shard. Two transitions on different instances hit different keys in the `instances` map and execute without blocking each other.

### Why Not RwLock\<HashMap\>?

A single `RwLock<HashMap>` serializes all writers and blocks readers during writes. With DashMap, a write to key A does not block a read from key B (unless they happen to land in the same shard). For a state machine database where many instances are transitioning concurrently, this is a significant performance difference.

### Why Not a Lock-Free Map?

True lock-free concurrent hash maps (like crossbeam's SkipMap) trade memory for concurrency and have different API ergonomics. DashMap is a practical middle ground: it uses fine-grained locks (not lock-free), but the sharding makes contention rare in practice. It also has a straightforward HashMap-like API.

## Arc Sharing

The engine and its dependencies are shared across async tasks using `Arc`:

```rust
pub struct Engine {
    catalog: Arc<MachineCatalog>,
    storage: Arc<dyn Storage>,
    timer_manager: Arc<TimerManager>,
    hook_executor: Arc<HookExecutor>,
    event_bus: Arc<EventBus>,
}
```

### AppState

The Axum server wraps the engine in an `AppState` that implements `Clone`:

```rust
#[derive(Clone)]
pub struct AppState {
    engine: Arc<Engine>,
    metrics: Arc<SmqlMetrics>,
}
```

Axum requires handler state to be `Clone`. Since every field is behind an `Arc`, cloning `AppState` is a cheap reference count increment, not a deep copy.

### EngineCallbackImpl

The hooks system needs a callback to the engine (so that hooks can spawn instances, query data, etc.). A naive design would be:

```rust
// This does not work
struct EngineCallbackImpl {
    engine: Arc<Engine>,  // Engine contains HookExecutor which contains callback...
}
```

This creates a self-referential `Arc` cycle. Instead, `EngineCallbackImpl` holds cloned `Arc` fields extracted from the engine:

```rust
struct EngineCallbackImpl {
    storage: Arc<dyn Storage>,
    catalog: Arc<MachineCatalog>,
    event_bus: Arc<EventBus>,
    // No Arc<Engine> — breaks the cycle
}
```

The `HookExecutor` stores its callback behind an `RwLock` so it can be set after construction:

```rust
pub struct HookExecutor {
    callback: RwLock<Option<Arc<dyn EngineCallback>>>,
}
```

The engine constructs the `HookExecutor` first (with `callback = None`), then constructs itself, then creates the `EngineCallbackImpl` with cloned Arc fields, and finally sets `hook_executor.callback` via `&self`. The `RwLock` makes this post-construction mutation safe.

## Optimistic Locking

Instances have a `version` field that starts at 1 and increments on every update:

```rust
pub struct Instance {
    pub id: InstanceId,
    pub machine: String,
    pub state: String,
    pub version: u64,
    pub data: BTreeMap<String, Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // ...
}
```

### How It Works

When the engine loads an instance to transition it, it records the current version. After computing the new state and data, it calls `update_instance` with the expected version:

```rust
// Load: version is 3
let instance = storage.get_instance(&id).await?;
let expected = instance.version; // 3

// ... compute new state, guards, mutations ...

instance.version += 1; // now 4
storage.update_instance(&instance, expected).await?;
```

Inside `update_instance`:

```rust
async fn update_instance(&self, instance: &Instance, expected_version: u64) -> Result<()> {
    let mut entry = self.instances.get_mut(&instance.id)
        .ok_or(SmqlError::NotFound)?;

    if entry.version != expected_version {
        return Err(SmqlError::Conflict {
            id: instance.id.clone(),
            expected: expected_version,
            actual: entry.version,
        });
    }

    *entry = instance.clone();
    // update indexes...
    Ok(())
}
```

### Race Scenario

Consider two concurrent transitions on the same instance (version 3):

| Time | Thread A | Thread B |
|---|---|---|
| T1 | Load instance (v3) | Load instance (v3) |
| T2 | Compute new state | Compute new state |
| T3 | update_instance(v3) succeeds, now v4 | |
| T4 | | update_instance(v3) fails: Conflict (expected 3, actual 4) |

Thread B gets a `Conflict` error and must retry. This is optimistic locking: no lock is held during computation (T2), only a cheap check at write time (T3/T4).

### When Version Checks Are Skipped

Two storage methods intentionally skip version checks:

- **`migrate_instances_state`**: Called by ALTER MACHINE to rename a state across all instances. The engine has already validated the schema change and needs to apply it atomically.
- **`bulk_update_instances`**: Called during schema evolution to update multiple instances. Same rationale.

These operations are performed under engine-level coordination, not in response to concurrent client requests.

## Putting It Together

A typical concurrent request flow:

1. HTTP request arrives, Axum extracts `AppState` (cheap clone of Arcs)
2. Handler calls `engine.transition(...)` through the `Arc<Engine>`
3. Engine reads the machine definition from `Arc<MachineCatalog>` (DashMap read)
4. Engine reads the instance from `Arc<dyn Storage>` (DashMap read in MemoryStorage)
5. Engine computes new state, evaluates guards, runs mutations
6. Engine writes the instance back with optimistic version check
7. If a concurrent write happened, step 6 fails with Conflict, client retries

No global lock is held at any point. Contention only occurs when two requests modify the same instance, and even then, the optimistic lock detects it cleanly rather than blocking.
