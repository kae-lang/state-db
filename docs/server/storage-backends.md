# Storage Backends

The SMQL server supports two storage backends. The backend is selected at startup with the `--storage` / `-s` flag.

## Quick Comparison

| Feature | Memory | RocksDB |
|---------|--------|---------|
| Persistence | None -- data lost on restart | Persistent to disk |
| Feature flag | None (always available) | Requires `rocksdb` feature |
| Performance | Fastest (DashMap) | Fast (LSM-tree, memory-mapped) |
| Concurrency | DashMap lock-free reads | RocksDB native concurrency |
| Use case | Development, testing, demos | Production, data durability |
| Startup | Instant | Opens/creates database directory |

## Memory Backend (default)

The memory backend stores all instances, trails, and indexes in `DashMap` concurrent hash maps. It requires no configuration and is always available.

```bash
smql serve --storage memory
```

This is the default when `--storage` is omitted:

```bash
smql serve
```

Data is lost when the process exits. This backend is ideal for:

- Local development and experimentation
- Automated tests
- Demos and prototyping
- Ephemeral workloads where persistence is not needed

## RocksDB Backend

The RocksDB backend provides persistent, crash-safe storage backed by Facebook's [RocksDB](https://rocksdb.org/) embedded key-value store.

### Enabling RocksDB

RocksDB is behind a Cargo feature flag. You must compile with the feature enabled:

```bash
# Build the CLI with RocksDB support
cargo build --release -p smql-cli --features rocksdb

# Build the server crate with RocksDB support
cargo build --release -p smql-server --features rocksdb
```

If you attempt to use a filesystem path for storage without the feature flag, the CLI exits with an error:

```
RocksDB storage requested ('./data') but the 'rocksdb' feature is not enabled.
Rebuild with: cargo build --features rocksdb
```

### Starting with RocksDB

Pass a filesystem path instead of `memory`:

```bash
# Create or open a RocksDB database at ./data
smql serve --storage ./data

# Absolute path
smql serve --storage /var/lib/smql/production
```

The directory is created automatically if it does not exist.

### Column Families

RocksDB organizes data into six column families, each serving a specific index or data type:

| Column Family | Key Format | Value | Purpose |
|---------------|-----------|-------|---------|
| `instances` | `{instance_id}` | Serialized `Instance` (JSON) | Primary instance store |
| `state_index` | `{machine}\x00{state}\x00{instance_id}` | `""` (empty) | Find instances by machine + state |
| `machine_index` | `{machine}\x00{instance_id}` | `""` (empty) | Find all instances of a machine |
| `trails` | `{instance_id}\x00{sequence:08}` | Serialized `TrailEntry` (JSON) | Audit trail entries |
| `parent_index` | `{parent_id}\x00{child_id}` | `""` (empty) | Parent-child composition lookup |
| `id_index` | `{instance_id}` | `{machine}` | Reverse lookup: ID to machine name |

Keys use `\x00` (NUL byte) as the separator in composite keys. This ensures correct lexicographic ordering and clean prefix scans.

### Atomic Operations

Multi-write operations (transitions that update instance + trail + indexes) use RocksDB `WriteBatch` for atomicity. Either all writes succeed or none do. This applies to:

- Spawn (instance + trail entry + all index entries)
- Transition (instance update + trail entry + index updates)
- Delete (instance + trail entries + all index entries)
- Schema migration (bulk instance state updates)

### Data Serialization

All values are serialized as JSON using `serde_json`. Keys are plain UTF-8 strings with NUL separators.

### Range Queries

Index lookups (e.g., "find all instances of SupportTicket in state open") use RocksDB range iteration with upper-bound options rather than prefix iterators. For a prefix like `SupportTicket\x00open\x00`, the iterator scans from that key up to `SupportTicket\x00open\x01` (the next byte after NUL).

### Data Directory Structure

RocksDB manages its own file layout inside the data directory:

```
./data/
  CURRENT
  IDENTITY
  LOCK
  LOG
  MANIFEST-000001
  OPTIONS-000001
  000001.sst
  000002.sst
  ...
```

Do not modify these files manually. Back up the entire directory if you need a snapshot.

## Switching Between Backends

The memory and RocksDB backends use the same `Storage` trait interface. Machines must be redefined after switching backends since machine definitions live in the in-memory catalog (not in storage).

A typical production workflow:

1. Develop with `--storage memory` for fast iteration.
2. Deploy with `--storage /var/lib/smql/data` for durability.
3. Define machines on startup (e.g., via `smql run machines.smql` or through the API).
