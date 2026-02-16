# Storage Trait & Implementations

The storage layer defines a trait that both `MemoryStorage` and `RocksDBStorage` implement. The engine only knows about the trait, never the concrete backend.

## The Storage Trait

```rust
pub trait Storage: Send + Sync {
    async fn store_instance(&self, instance: &Instance) -> Result<()>;
    async fn get_instance(&self, id: &InstanceId) -> Result<Option<Instance>>;
    async fn update_instance(&self, instance: &Instance, expected_version: u64) -> Result<()>;
    async fn delete_instance(&self, id: &InstanceId) -> Result<()>;
    async fn find_by_machine(&self, machine: &str) -> Result<Vec<Instance>>;
    async fn find_by_state(&self, machine: &str, state: &str) -> Result<Vec<Instance>>;
    async fn append_trail(&self, id: &InstanceId, entry: TrailEntry) -> Result<()>;
    async fn get_trail(&self, id: &InstanceId) -> Result<Vec<TrailEntry>>;
    async fn find_children(
        &self,
        parent_id: &InstanceId,
        child_machine: &str,
    ) -> Result<Vec<Instance>>;
    async fn migrate_instances_state(
        &self,
        machine: &str,
        from: &str,
        to: &str,
    ) -> Result<u64>;
    async fn bulk_update_instances(&self, instances: &[Instance]) -> Result<()>;
}
```

### Method Semantics

| Method | Purpose | Notes |
|---|---|---|
| `store_instance` | Insert a new instance | Fails if ID already exists |
| `get_instance` | Fetch by ID | Returns `None` if not found |
| `update_instance` | Update with optimistic locking | Checks `expected_version`, returns `Conflict` on mismatch |
| `delete_instance` | Remove instance and all related data | Cascades to trail entries and index entries |
| `find_by_machine` | All instances of a machine type | Used by queries and schema migration |
| `find_by_state` | Instances in a specific state of a machine | State index lookup, not full scan |
| `append_trail` | Add an audit trail entry | Entries are append-only |
| `get_trail` | Full audit trail for an instance | Ordered by sequence number |
| `find_children` | Instances spawned by a parent | Used for composition (ALL/ANY predicates, CASCADE) |
| `migrate_instances_state` | Bulk rename a state across all instances | Used by ALTER MACHINE. Skips version checks. |
| `bulk_update_instances` | Bulk write multiple instances | Used by schema migration. Skips version checks. |

The last two methods (`migrate_instances_state` and `bulk_update_instances`) intentionally skip optimistic version checks. They are used for schema migration operations where the engine has already validated the change and needs to apply it atomically across many instances.

## MemoryStorage

`MemoryStorage` uses four `DashMap` instances for concurrent access:

```rust
pub struct MemoryStorage {
    /// Primary store: InstanceId -> Instance
    instances: DashMap<InstanceId, Instance>,

    /// State index: (machine, state) -> HashSet<InstanceId>
    state_index: DashMap<(String, String), HashSet<InstanceId>>,

    /// Audit trail: InstanceId -> Vec<TrailEntry>
    trails: DashMap<InstanceId, Vec<TrailEntry>>,

    /// Parent index: parent InstanceId -> Vec<child InstanceId>
    parent_index: DashMap<InstanceId, Vec<InstanceId>>,
}
```

### Index Maintenance

When an instance is stored or its state changes, the state index must be updated. This is a two-step operation:

1. Remove the instance ID from the old `(machine, old_state)` entry
2. Insert the instance ID into the new `(machine, new_state)` entry

Both steps happen within `update_instance`. The DashMap sharding means different machines and states can be updated concurrently without contention.

### find_children

The parent index maps a parent instance ID to a list of child instance IDs. When the engine spawns a child instance (via `__spawn` in a MUTATE clause), it records the parent-child relationship. `find_children` filters by `child_machine` after looking up the parent's children.

## RocksDB Storage

The RocksDB backend is feature-gated behind the `rocksdb` feature flag. It stores data in 6 column families:

| Column Family | Key Format | Value |
|---|---|---|
| `instances` | `{instance_id}` | JSON-serialized `Instance` |
| `state_index` | `{machine}\x00{state}\x00{instance_id}` | empty (presence = membership) |
| `machine_index` | `{machine}\x00{instance_id}` | empty |
| `trails` | `{instance_id}\x00{sequence:08}` | JSON-serialized `TrailEntry` |
| `parent_index` | `{parent_id}\x00{child_id}` | empty |
| `id_index` | `{instance_id}` | `{machine}` |

### Composite Key Design

Keys use NUL (`\x00`) as a separator. This byte never appears in valid ULID strings or machine/state names, making it a safe delimiter. The format enables range scans:

```rust
// Find all instances of "SupportTicket" in state "open":
let prefix = b"SupportTicket\x00open\x00";
let upper_bound = b"SupportTicket\x00open\x01"; // \x01 is one past \x00

let mut opts = ReadOptions::default();
opts.set_iterate_upper_bound(upper_bound);
let iter = db.iterator_cf_opt(cf, opts, IteratorMode::From(prefix, Forward));
```

**Why not `prefix_iterator_cf`?** RocksDB's prefix iterator requires a `SliceTransform` extractor configured at DB open time. A no-op extractor causes the iterator to return zero results. Using `iterator_cf_opt` with an explicit upper bound is reliable and does not require special configuration.

### Atomic Writes with WriteBatch

Operations that touch multiple column families use `WriteBatch` for atomicity:

```rust
let mut batch = WriteBatch::default();

// Update the instance
batch.put_cf(instances_cf, &id, &serialized);

// Remove old state index entry
batch.delete_cf(state_index_cf, &old_key);

// Add new state index entry
batch.put_cf(state_index_cf, &new_key, b"");

// Append trail entry
batch.put_cf(trails_cf, &trail_key, &trail_serialized);

db.write(batch)?;
```

This ensures that if the process crashes mid-write, either all changes are visible or none are. Without `WriteBatch`, a crash between the instance update and the state index update would leave the index inconsistent.

### Serialization

All values are serialized with `serde_json`. This is simple, human-debuggable (you can inspect the database with `ldb`), and sufficient for the current scale. Binary serialization (e.g., bincode, MessagePack) is a future optimization if JSON becomes a bottleneck.

### Migration Methods

`migrate_instances_state` scans the `machine_index` column family for all instances of the specified machine, deserializes each, updates the state field, and writes them back. It also updates the `state_index` entries. This is done in a single `WriteBatch`.

`bulk_update_instances` takes a slice of pre-modified instances and writes them all in one batch. Both methods skip version checks because they are called during schema evolution (ALTER MACHINE) after the engine has already validated the operation.

## Choosing a Backend

- **MemoryStorage**: Fast, zero setup, no persistence. Good for tests, development, and ephemeral workloads.
- **RocksDBStorage**: Persistent, crash-safe, handles datasets larger than memory. Requires the `rocksdb` feature and librocksdb.

The engine does not know or care which backend is in use. It holds an `Arc<dyn Storage>` and calls trait methods.
