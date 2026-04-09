use async_trait::async_trait;
use chrono::Utc;
use rocksdb::{
    BlockBasedOptions, Cache, ColumnFamilyDescriptor, DBCompressionType, DBWithThreadMode,
    IteratorMode, MultiThreaded, Options, ReadOptions, WriteBatchWithTransaction,
};
use smql_ast::value::Value;
use smql_ast::{SmqlError, SmqlResult};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::instance::{
    Filter, Instance, InstanceId, Mutation, StoredTimer, TrailEntry, TrailFilter,
};
use crate::traits::Storage;

type DB = DBWithThreadMode<MultiThreaded>;

const CF_INSTANCES: &str = "instances";
const CF_STATE_INDEX: &str = "state_index";
const CF_MACHINE_INDEX: &str = "machine_index";
const CF_TRAILS: &str = "trails";
const CF_PARENT_INDEX: &str = "parent_index";
const CF_ID_INDEX: &str = "id_index";
const CF_TIMERS: &str = "timers";
const CF_IDEMPOTENCY: &str = "idempotency";
const CF_EVENTS: &str = "events";

const SEP: u8 = 0x00;

/// RocksDB-backed persistent storage backend.
///
/// Uses 8 column families: instances, state_index, machine_index, trails,
/// parent_index, id_index, timers, and idempotency. Atomic writes use WriteBatch.
///
/// **Concurrency note:** Operations like `store_instance`, `update_instance`,
/// and `transition_instance` use a read-check-write pattern that is not
/// transactionally atomic. The engine serializes writes through its own
/// coordination, but direct concurrent access to the storage layer can
/// experience TOCTOU races. For true multi-writer safety, migrate to
/// `TransactionDB` with `get_for_update`.
pub struct RocksDBStorage {
    db: Arc<DB>,
}

impl RocksDBStorage {
    /// Open (or create) a RocksDB database at the given path with 7 column families.
    pub fn open(path: impl AsRef<Path>) -> SmqlResult<Self> {
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);
        db_opts.increase_parallelism(4);
        db_opts.set_max_background_jobs(4);

        // Shared 128MB LRU block cache across all column families
        let cache = Cache::new_lru_cache(128 * 1024 * 1024);

        // Helper to create CF options with bloom filter + shared cache
        let make_cf_opts = |compression: DBCompressionType| -> Options {
            let mut cf_opts = Options::default();
            let mut block_opts = BlockBasedOptions::default();
            block_opts.set_bloom_filter(10.0, false);
            block_opts.set_block_cache(&cache);
            cf_opts.set_block_based_table_factory(&block_opts);
            cf_opts.set_compression_type(compression);
            cf_opts
        };

        let cf_descriptors = vec![
            ColumnFamilyDescriptor::new(CF_INSTANCES, make_cf_opts(DBCompressionType::Lz4)),
            ColumnFamilyDescriptor::new(CF_STATE_INDEX, make_cf_opts(DBCompressionType::None)),
            ColumnFamilyDescriptor::new(CF_MACHINE_INDEX, make_cf_opts(DBCompressionType::None)),
            ColumnFamilyDescriptor::new(CF_TRAILS, make_cf_opts(DBCompressionType::Zstd)),
            ColumnFamilyDescriptor::new(CF_PARENT_INDEX, make_cf_opts(DBCompressionType::None)),
            ColumnFamilyDescriptor::new(CF_ID_INDEX, make_cf_opts(DBCompressionType::None)),
            ColumnFamilyDescriptor::new(CF_TIMERS, Options::default()),
            ColumnFamilyDescriptor::new(CF_IDEMPOTENCY, Options::default()),
            ColumnFamilyDescriptor::new(CF_EVENTS, make_cf_opts(DBCompressionType::Zstd)),
        ];

        let db = DB::open_cf_descriptors(&db_opts, path.as_ref(), cf_descriptors)
            .map_err(|e| SmqlError::storage(format!("Failed to open RocksDB: {}", e)))?;

        Ok(Self { db: Arc::new(db) })
    }

    /// Destroy the database at the given path (for test cleanup).
    pub fn destroy(path: impl AsRef<Path>) -> SmqlResult<()> {
        DB::destroy(&Options::default(), path.as_ref())
            .map_err(|e| SmqlError::storage(format!("Failed to destroy RocksDB: {}", e)))
    }

    // --- Key helpers ---

    fn make_key(parts: &[&[u8]]) -> Vec<u8> {
        let total_len: usize =
            parts.iter().map(|p| p.len()).sum::<usize>() + parts.len().saturating_sub(1);
        let mut key = Vec::with_capacity(total_len);
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                key.push(SEP);
            }
            key.extend_from_slice(part);
        }
        key
    }

    fn instance_key(machine: &str, id: &str) -> Vec<u8> {
        Self::make_key(&[machine.as_bytes(), id.as_bytes()])
    }

    fn state_index_key(machine: &str, state: &str, id: &str) -> Vec<u8> {
        Self::make_key(&[machine.as_bytes(), state.as_bytes(), id.as_bytes()])
    }

    fn machine_index_key(machine: &str, id: &str) -> Vec<u8> {
        Self::make_key(&[machine.as_bytes(), id.as_bytes()])
    }

    fn trail_key(machine: &str, id: &str, seq: u64) -> Vec<u8> {
        let seq_str = format!("{:08x}", seq);
        Self::make_key(&[machine.as_bytes(), id.as_bytes(), seq_str.as_bytes()])
    }

    fn parent_index_key(parent_id: &str, child_machine: &str, child_id: &str) -> Vec<u8> {
        Self::make_key(&[
            parent_id.as_bytes(),
            child_machine.as_bytes(),
            child_id.as_bytes(),
        ])
    }

    fn prefix_key(parts: &[&[u8]]) -> Vec<u8> {
        let mut key = Self::make_key(parts);
        key.push(SEP);
        key
    }

    // --- Serialization ---

    fn serialize_instance(instance: &Instance) -> SmqlResult<Vec<u8>> {
        serde_json::to_vec(instance)
            .map_err(|e| SmqlError::storage(format!("Serialize instance: {}", e)))
    }

    fn deserialize_instance(bytes: &[u8]) -> SmqlResult<Instance> {
        serde_json::from_slice(bytes)
            .map_err(|e| SmqlError::storage(format!("Deserialize instance: {}", e)))
    }

    fn serialize_trail_entry(entry: &TrailEntry) -> SmqlResult<Vec<u8>> {
        serde_json::to_vec(entry)
            .map_err(|e| SmqlError::storage(format!("Serialize trail entry: {}", e)))
    }

    fn deserialize_trail_entry(bytes: &[u8]) -> SmqlResult<TrailEntry> {
        serde_json::from_slice(bytes)
            .map_err(|e| SmqlError::storage(format!("Deserialize trail entry: {}", e)))
    }

    // --- Internal helpers ---

    /// Load an instance by ID, using id_index to find the machine name.
    fn load_instance(&self, id: &str) -> SmqlResult<Option<Instance>> {
        let cf_id = self.cf(CF_ID_INDEX)?;
        match self
            .db
            .get_cf(&cf_id, id.as_bytes())
            .map_err(|e| SmqlError::storage(e.to_string()))?
        {
            Some(machine_bytes) => {
                let machine = String::from_utf8(machine_bytes.to_vec())
                    .map_err(|e| SmqlError::storage(format!("Invalid UTF-8 in id_index: {}", e)))?;
                let cf_inst = self.cf(CF_INSTANCES)?;
                let key = Self::instance_key(&machine, id);
                match self
                    .db
                    .get_cf(&cf_inst, &key)
                    .map_err(|e| SmqlError::storage(e.to_string()))?
                {
                    Some(bytes) => Ok(Some(Self::deserialize_instance(&bytes)?)),
                    None => Ok(None),
                }
            }
            None => Ok(None),
        }
    }

    /// Scan all keys with a given prefix in a column family, returning raw key-value pairs.
    /// Uses range iteration with an upper bound to efficiently scan by prefix.
    fn scan_prefix(&self, cf_name: &str, prefix: &[u8]) -> SmqlResult<Vec<(Vec<u8>, Vec<u8>)>> {
        let cf = self.cf(cf_name)?;

        // Compute upper bound: increment last byte to get the exclusive end of prefix range.
        let upper_bound = Self::prefix_upper_bound(prefix);

        let mut read_opts = ReadOptions::default();
        read_opts.fill_cache(false);
        if let Some(ref ub) = upper_bound {
            read_opts.set_iterate_upper_bound(ub.clone());
        }

        let iter = self.db.iterator_cf_opt(
            &cf,
            read_opts,
            IteratorMode::From(prefix, rocksdb::Direction::Forward),
        );

        let mut results = Vec::new();
        for item in iter {
            let (key, value) = item.map_err(|e| SmqlError::storage(e.to_string()))?;
            if !key.starts_with(prefix) {
                break;
            }
            results.push((key.to_vec(), value.to_vec()));
        }
        Ok(results)
    }

    /// Compute an exclusive upper bound for a prefix scan.
    /// Increments the last byte; returns None if the prefix is all 0xFF.
    fn prefix_upper_bound(prefix: &[u8]) -> Option<Vec<u8>> {
        let mut upper = prefix.to_vec();
        for i in (0..upper.len()).rev() {
            if upper[i] < 0xFF {
                upper[i] += 1;
                upper.truncate(i + 1);
                return Some(upper);
            }
        }
        None // all 0xFF — scan to end
    }

    /// Scan state_index for a given machine+state prefix, returning instance IDs.
    fn scan_state_index(&self, machine: &str, state: &str) -> SmqlResult<Vec<String>> {
        let prefix = Self::prefix_key(&[machine.as_bytes(), state.as_bytes()]);
        let pairs = self.scan_prefix(CF_STATE_INDEX, &prefix)?;
        let mut ids = Vec::new();
        for (key, _) in pairs {
            // Key format: machine\0state\0id
            if let Some(id) = self.extract_last_segment(&key) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    /// Scan machine_index for all instance IDs of a machine.
    fn scan_machine_index(&self, machine: &str) -> SmqlResult<Vec<String>> {
        let prefix = Self::prefix_key(&[machine.as_bytes()]);
        let pairs = self.scan_prefix(CF_MACHINE_INDEX, &prefix)?;
        let mut ids = Vec::new();
        for (key, _) in pairs {
            if let Some(id) = self.extract_last_segment(&key) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    /// Extract the last NUL-separated segment from a key as a String.
    fn extract_last_segment(&self, key: &[u8]) -> Option<String> {
        let pos = key.iter().rposition(|&b| b == SEP)?;
        let segment = &key[pos + 1..];
        String::from_utf8(segment.to_vec()).ok()
    }

    /// Get a column family handle, returning an error instead of panicking.
    fn cf(&self, name: &str) -> SmqlResult<&rocksdb::ColumnFamily> {
        self.db.cf_handle(name).ok_or_else(|| {
            SmqlError::storage(format!(
                "Column family '{}' not found — database may be corrupted",
                name
            ))
        })
    }

    fn apply_mutations(instance: &mut Instance, mutations: &[Mutation]) {
        for mutation in mutations {
            match mutation {
                Mutation::SetField(field, value) => {
                    instance.data.insert(field.clone(), value.clone());
                }
                Mutation::RemoveField(field) => {
                    instance.data.remove(field);
                }
                Mutation::IncrementField(field, delta) => {
                    if let Some(Value::Int(v)) = instance.data.get_mut(field) {
                        *v += delta;
                    }
                }
                Mutation::AppendToList(field, value) => {
                    if let Some(Value::List(list)) = instance.data.get_mut(field) {
                        list.push(value.clone());
                    }
                }
                Mutation::SetTag(key, value) => {
                    instance.tags.insert(key.clone(), value.clone());
                }
            }
        }
        instance.updated_at = Utc::now();
    }
}

#[async_trait]
impl Storage for RocksDBStorage {
    async fn store_instance(&self, instance: &Instance) -> SmqlResult<()> {
        let id_str = instance.id.as_str();

        // Check for duplicate
        if self.load_instance(&id_str)?.is_some() {
            return Err(SmqlError::Conflict {
                message: format!("Instance '{}' already exists", id_str),
                hint: None,
            });
        }

        let cf_inst = self.cf(CF_INSTANCES)?;
        let cf_state = self.cf(CF_STATE_INDEX)?;
        let cf_machine = self.cf(CF_MACHINE_INDEX)?;
        let cf_id = self.cf(CF_ID_INDEX)?;
        let cf_parent = self.cf(CF_PARENT_INDEX)?;

        let bytes = Self::serialize_instance(instance)?;

        let mut batch = WriteBatchWithTransaction::<false>::default();
        batch.put_cf(
            &cf_inst,
            Self::instance_key(&instance.machine, &id_str),
            &bytes,
        );
        batch.put_cf(
            &cf_state,
            Self::state_index_key(&instance.machine, &instance.state, &id_str),
            b"",
        );
        batch.put_cf(
            &cf_machine,
            Self::machine_index_key(&instance.machine, &id_str),
            b"",
        );
        batch.put_cf(&cf_id, id_str.as_bytes(), instance.machine.as_bytes());

        if let Some(parent_id) = &instance.parent_id {
            batch.put_cf(
                &cf_parent,
                Self::parent_index_key(&parent_id.as_str(), &instance.machine, &id_str),
                b"",
            );
        }

        self.db
            .write(batch)
            .map_err(|e| SmqlError::storage(e.to_string()))?;

        Ok(())
    }

    async fn spawn_instance(
        &self,
        instance: &Instance,
        trail_entry: &TrailEntry,
        event: Option<&crate::instance::StoredEvent>,
        idempotency_key: Option<(&str, &[u8], chrono::DateTime<chrono::Utc>)>,
    ) -> SmqlResult<()> {
        let id_str = instance.id.as_str();

        if self.load_instance(&id_str)?.is_some() {
            return Err(SmqlError::Conflict {
                message: format!("Instance '{}' already exists", id_str),
                hint: None,
            });
        }

        let cf_inst = self.cf(CF_INSTANCES)?;
        let cf_state = self.cf(CF_STATE_INDEX)?;
        let cf_machine = self.cf(CF_MACHINE_INDEX)?;
        let cf_id = self.cf(CF_ID_INDEX)?;
        let cf_parent = self.cf(CF_PARENT_INDEX)?;
        let cf_trails = self.cf(CF_TRAILS)?;

        let inst_bytes = Self::serialize_instance(instance)?;
        let trail_bytes = Self::serialize_trail_entry(trail_entry)?;

        let mut batch = WriteBatchWithTransaction::<false>::default();

        // Instance + indices
        batch.put_cf(&cf_inst, Self::instance_key(&instance.machine, &id_str), &inst_bytes);
        batch.put_cf(&cf_state, Self::state_index_key(&instance.machine, &instance.state, &id_str), b"");
        batch.put_cf(&cf_machine, Self::machine_index_key(&instance.machine, &id_str), b"");
        batch.put_cf(&cf_id, id_str.as_bytes(), instance.machine.as_bytes());

        if let Some(parent_id) = &instance.parent_id {
            batch.put_cf(&cf_parent, Self::parent_index_key(&parent_id.as_str(), &instance.machine, &id_str), b"");
        }

        // Trail entry
        batch.put_cf(&cf_trails, Self::trail_key(&instance.machine, &id_str, trail_entry.sequence), &trail_bytes);

        // Event
        if let Some(evt) = event {
            let cf_events = self.cf(CF_EVENTS)?;
            let event_bytes = serde_json::to_vec(evt)
                .map_err(|e| SmqlError::storage(format!("Serialize event: {}", e)))?;
            batch.put_cf(&cf_events, evt.id.as_bytes(), &event_bytes);
        }

        // Idempotency key
        if let Some((key, response, expires_at)) = idempotency_key {
            let cf_idemp = self.cf(CF_IDEMPOTENCY)?;
            let entry = serde_json::json!({
                "response": serde_json::from_slice::<serde_json::Value>(response).unwrap_or(serde_json::Value::Null),
                "expires_at": expires_at.to_rfc3339(),
            });
            let entry_bytes = serde_json::to_vec(&entry)
                .map_err(|e| SmqlError::storage(format!("Serialize idempotency: {}", e)))?;
            batch.put_cf(&cf_idemp, key.as_bytes(), &entry_bytes);
        }

        self.db.write(batch).map_err(|e| SmqlError::storage(e.to_string()))?;
        Ok(())
    }

    async fn get_instance(&self, id: &InstanceId) -> SmqlResult<Option<Instance>> {
        self.load_instance(&id.as_str())
    }

    async fn find_instances(&self, machine: &str, filter: &Filter) -> SmqlResult<Vec<Instance>> {
        let candidate_ids: Vec<String> = if let Some(state) = &filter.state {
            self.scan_state_index(machine, state)?
        } else if let Some(states) = &filter.states {
            let mut ids = Vec::new();
            for state in states {
                ids.extend(self.scan_state_index(machine, state)?);
            }
            ids
        } else {
            self.scan_machine_index(machine)?
        };

        let cf_inst = self.cf(CF_INSTANCES)?;
        let mut results: Vec<Instance> = Vec::new();
        for id in &candidate_ids {
            let key = Self::instance_key(machine, id);
            if let Some(bytes) = self
                .db
                .get_cf(&cf_inst, &key)
                .map_err(|e| SmqlError::storage(e.to_string()))?
            {
                let inst = Self::deserialize_instance(&bytes)?;
                if inst.machine == machine {
                    if let Some(pred) = &filter.predicate {
                        if pred.matches(&inst.data) {
                            results.push(inst);
                        }
                    } else {
                        results.push(inst);
                    }
                }
            }
        }

        // Sort by ID for deterministic cursor-based pagination
        results.sort_by(|a, b| a.id.as_str().cmp(&b.id.as_str()));

        // Apply cursor filter: only instances with ID > after_id
        if let Some(after_id) = &filter.after_id {
            results.retain(|inst| inst.id.as_str().as_str() > after_id.as_str());
        }

        // Apply offset and limit
        if let Some(offset) = filter.offset {
            if offset < results.len() {
                results = results.into_iter().skip(offset).collect();
            } else {
                results.clear();
            }
        }
        if let Some(limit) = filter.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    /// Update an instance's data fields via mutations.
    ///
    /// **Concurrency note:** This implementation uses a read-check-write pattern
    /// that is not fully atomic under concurrent access. The engine's single-writer
    /// design (via its async task model) provides practical safety, but direct
    /// concurrent calls to this method can experience TOCTOU races. A future
    /// migration to RocksDB's `TransactionDB` with `get_for_update` would provide
    /// true serializable isolation.
    async fn update_instance(
        &self,
        id: &InstanceId,
        expected_version: u64,
        mutations: &[Mutation],
    ) -> SmqlResult<()> {
        let id_str = id.as_str();
        let mut instance = self
            .load_instance(&id_str)?
            .ok_or_else(|| SmqlError::not_found("Instance", &id_str))?;

        if instance.version != expected_version {
            return Err(SmqlError::Conflict {
                message: format!(
                    "Version conflict for instance '{}': expected {}, got {}",
                    id_str, expected_version, instance.version
                ),
                hint: Some("Re-read the instance and retry".into()),
            });
        }

        Self::apply_mutations(&mut instance, mutations);
        instance.version += 1;

        let cf_inst = self.cf(CF_INSTANCES)?;
        let bytes = Self::serialize_instance(&instance)?;
        self.db
            .put_cf(
                &cf_inst,
                Self::instance_key(&instance.machine, &id_str),
                &bytes,
            )
            .map_err(|e| SmqlError::storage(e.to_string()))?;

        Ok(())
    }

    /// Transition an instance to a new state atomically.
    ///
    /// **Concurrency note:** The version check and write use a read-check-write
    /// pattern. While the WriteBatch ensures the state+trail+index updates are
    /// atomic, the version check is not transactionally bound to the write.
    /// The engine serializes transitions through its async task model, but
    /// direct concurrent access should migrate to `TransactionDB`.
    async fn transition_instance(
        &self,
        id: &InstanceId,
        expected_version: u64,
        new_state: &str,
        mutations: &[Mutation],
        trail_entry: TrailEntry,
    ) -> SmqlResult<Instance> {
        let id_str = id.as_str();
        let mut instance = self
            .load_instance(&id_str)?
            .ok_or_else(|| SmqlError::not_found("Instance", &id_str))?;

        if instance.version != expected_version {
            return Err(SmqlError::Conflict {
                message: format!(
                    "Version conflict for instance '{}': expected {}, got {}",
                    id_str, expected_version, instance.version
                ),
                hint: Some("Re-read the instance and retry".into()),
            });
        }

        let old_state = instance.state.clone();
        let machine = instance.machine.clone();

        Self::apply_mutations(&mut instance, mutations);
        instance.state = new_state.to_string();
        instance.state_entered_at = Utc::now();
        instance.version += 1;
        instance.trail_length += 1;

        let cf_inst = self.cf(CF_INSTANCES)?;
        let cf_state = self.cf(CF_STATE_INDEX)?;
        let cf_trails = self.cf(CF_TRAILS)?;

        let mut batch = WriteBatchWithTransaction::<false>::default();

        // Update instance
        let inst_bytes = Self::serialize_instance(&instance)?;
        batch.put_cf(&cf_inst, Self::instance_key(&machine, &id_str), &inst_bytes);

        // Update state index
        batch.delete_cf(
            &cf_state,
            Self::state_index_key(&machine, &old_state, &id_str),
        );
        batch.put_cf(
            &cf_state,
            Self::state_index_key(&machine, new_state, &id_str),
            b"",
        );

        // Append trail entry
        let trail_bytes = Self::serialize_trail_entry(&trail_entry)?;
        batch.put_cf(
            &cf_trails,
            Self::trail_key(&machine, &id_str, trail_entry.sequence),
            &trail_bytes,
        );

        self.db
            .write(batch)
            .map_err(|e| SmqlError::storage(e.to_string()))?;

        Ok(instance)
    }

    async fn delete_instance(&self, id: &InstanceId) -> SmqlResult<()> {
        let id_str = id.as_str();
        let instance = self
            .load_instance(&id_str)?
            .ok_or_else(|| SmqlError::not_found("Instance", &id_str))?;

        let cf_inst = self.cf(CF_INSTANCES)?;
        let cf_state = self.cf(CF_STATE_INDEX)?;
        let cf_machine = self.cf(CF_MACHINE_INDEX)?;
        let cf_id = self.cf(CF_ID_INDEX)?;
        let cf_parent = self.cf(CF_PARENT_INDEX)?;
        let cf_trails = self.cf(CF_TRAILS)?;

        let mut batch = WriteBatchWithTransaction::<false>::default();

        batch.delete_cf(&cf_inst, Self::instance_key(&instance.machine, &id_str));
        batch.delete_cf(
            &cf_state,
            Self::state_index_key(&instance.machine, &instance.state, &id_str),
        );
        batch.delete_cf(
            &cf_machine,
            Self::machine_index_key(&instance.machine, &id_str),
        );
        batch.delete_cf(&cf_id, id_str.as_bytes());

        // Delete from parent index if child
        if let Some(parent_id) = &instance.parent_id {
            batch.delete_cf(
                &cf_parent,
                Self::parent_index_key(&parent_id.as_str(), &instance.machine, &id_str),
            );
        }

        // Delete all trail entries for this instance
        let trail_prefix = Self::prefix_key(&[instance.machine.as_bytes(), id_str.as_bytes()]);
        let trail_pairs = self.scan_prefix(CF_TRAILS, &trail_prefix)?;
        for (key, _) in trail_pairs {
            batch.delete_cf(&cf_trails, &key);
        }

        // Delete all timers for this instance
        let cf_timers = self.cf(CF_TIMERS)?;
        let timer_prefix = Self::prefix_key(&[id_str.as_bytes()]);
        let timer_pairs = self.scan_prefix(CF_TIMERS, &timer_prefix)?;
        for (key, _) in timer_pairs {
            batch.delete_cf(&cf_timers, &key);
        }

        self.db
            .write(batch)
            .map_err(|e| SmqlError::storage(e.to_string()))?;

        Ok(())
    }

    async fn count_by_state(&self, machine: &str) -> SmqlResult<HashMap<String, usize>> {
        let mut counts: HashMap<String, usize> = HashMap::new();

        // Scan state_index keys directly: format is machine\0state\0id
        // This avoids loading and deserializing every instance.
        let prefix = Self::prefix_key(&[machine.as_bytes()]);
        let pairs = self.scan_prefix(CF_STATE_INDEX, &prefix)?;
        let machine_prefix_len = machine.len() + 1; // machine + SEP

        for (key, _) in pairs {
            // Key: machine\0state\0id — extract the state (middle segment)
            let remainder = &key[machine_prefix_len..];
            if let Some(sep_pos) = remainder.iter().position(|&b| b == SEP) {
                if let Ok(state) = std::str::from_utf8(&remainder[..sep_pos]) {
                    *counts.entry(state.to_string()).or_insert(0) += 1;
                }
            }
        }

        Ok(counts)
    }

    async fn append_trail_entry(&self, entry: &TrailEntry) -> SmqlResult<()> {
        let cf_trails = self.cf(CF_TRAILS)?;
        let id_str = entry.instance_id.as_str();
        let key = Self::trail_key(&entry.machine, &id_str, entry.sequence);
        let bytes = Self::serialize_trail_entry(entry)?;
        self.db
            .put_cf(&cf_trails, &key, &bytes)
            .map_err(|e| SmqlError::storage(e.to_string()))?;
        Ok(())
    }

    async fn get_trail(&self, id: &InstanceId) -> SmqlResult<Vec<TrailEntry>> {
        let id_str = id.as_str();

        // We need the machine name to build the trail prefix.
        // Look it up from id_index.
        let cf_id = self.cf(CF_ID_INDEX)?;
        let machine = match self
            .db
            .get_cf(&cf_id, id_str.as_bytes())
            .map_err(|e| SmqlError::storage(e.to_string()))?
        {
            Some(bytes) => String::from_utf8(bytes.to_vec())
                .map_err(|e| SmqlError::storage(format!("Invalid UTF-8 in id_index: {}", e)))?,
            None => return Err(SmqlError::not_found("Trail", &id_str)),
        };

        let prefix = Self::prefix_key(&[machine.as_bytes(), id_str.as_bytes()]);
        let pairs = self.scan_prefix(CF_TRAILS, &prefix)?;

        let mut entries = Vec::new();
        for (_, value) in pairs {
            entries.push(Self::deserialize_trail_entry(&value)?);
        }

        Ok(entries)
    }

    async fn get_trails_batch(
        &self,
        ids: &[InstanceId],
    ) -> SmqlResult<HashMap<String, Vec<TrailEntry>>> {
        let cf_id = self.cf(CF_ID_INDEX)?;
        let mut result = HashMap::with_capacity(ids.len());

        for id in ids {
            let id_str = id.as_str();
            // Look up machine from id_index
            let machine = match self
                .db
                .get_cf(&cf_id, id_str.as_bytes())
                .map_err(|e| SmqlError::storage(e.to_string()))?
            {
                Some(bytes) => String::from_utf8(bytes.to_vec())
                    .map_err(|e| SmqlError::storage(format!("Invalid UTF-8: {}", e)))?,
                None => {
                    result.insert(id_str, Vec::new());
                    continue;
                }
            };

            let prefix = Self::prefix_key(&[machine.as_bytes(), id_str.as_bytes()]);
            let pairs = self.scan_prefix(CF_TRAILS, &prefix)?;
            let mut entries = Vec::with_capacity(pairs.len());
            for (_, value) in pairs {
                entries.push(Self::deserialize_trail_entry(&value)?);
            }
            result.insert(id_str, entries);
        }

        Ok(result)
    }

    async fn query_trails(
        &self,
        machine: &str,
        filter: &TrailFilter,
    ) -> SmqlResult<Vec<TrailEntry>> {
        let ids = self.scan_machine_index(machine)?;
        let mut results = Vec::new();

        for id in &ids {
            let prefix = Self::prefix_key(&[machine.as_bytes(), id.as_bytes()]);
            let pairs = self.scan_prefix(CF_TRAILS, &prefix)?;

            for (_, value) in pairs {
                let entry = Self::deserialize_trail_entry(&value)?;
                let matches = filter
                    .from_state
                    .as_ref()
                    .is_none_or(|s| entry.from_state == *s)
                    && filter
                        .to_state
                        .as_ref()
                        .is_none_or(|s| entry.to_state == *s)
                    && filter
                        .actor
                        .as_ref()
                        .is_none_or(|a| entry.actor.as_ref() == Some(a))
                    && filter.after.is_none_or(|t| entry.timestamp > t)
                    && filter.before.is_none_or(|t| entry.timestamp < t);

                if matches {
                    results.push(entry);
                }
            }
        }

        results.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        if let Some(limit) = filter.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    async fn find_children(
        &self,
        parent_id: &InstanceId,
        child_machine: Option<&str>,
    ) -> SmqlResult<Vec<Instance>> {
        let parent_str = parent_id.as_str();

        let prefix = if let Some(machine) = child_machine {
            Self::prefix_key(&[parent_str.as_bytes(), machine.as_bytes()])
        } else {
            Self::prefix_key(&[parent_str.as_bytes()])
        };

        let pairs = self.scan_prefix(CF_PARENT_INDEX, &prefix)?;
        let mut results = Vec::new();

        for (key, _) in pairs {
            // Extract child_id from key: parent_id\0child_machine\0child_id
            if let Some(child_id) = self.extract_last_segment(&key) {
                if let Some(inst) = self.load_instance(&child_id)? {
                    results.push(inst);
                }
            }
        }

        Ok(results)
    }

    async fn get_parent(&self, child_id: &InstanceId) -> SmqlResult<Option<Instance>> {
        let child = self.get_instance(child_id).await?;
        match child {
            Some(inst) => {
                if let Some(parent_id) = &inst.parent_id {
                    self.get_instance(parent_id).await
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    async fn migrate_instances_state(
        &self,
        machine: &str,
        from_state: &str,
        to_state: &str,
    ) -> SmqlResult<u64> {
        let ids = self.scan_state_index(machine, from_state)?;

        let cf_inst = self.cf(CF_INSTANCES)?;
        let cf_state = self.cf(CF_STATE_INDEX)?;

        let mut batch = WriteBatchWithTransaction::<false>::default();
        let mut count = 0u64;

        for id in &ids {
            let key = Self::instance_key(machine, id);
            if let Some(bytes) = self
                .db
                .get_cf(&cf_inst, &key)
                .map_err(|e| SmqlError::storage(e.to_string()))?
            {
                let mut inst = Self::deserialize_instance(&bytes)?;
                if inst.machine == machine && inst.state == from_state {
                    inst.state = to_state.to_string();
                    inst.state_entered_at = Utc::now();
                    inst.updated_at = Utc::now();
                    inst.version += 1;

                    let new_bytes = Self::serialize_instance(&inst)?;
                    batch.put_cf(&cf_inst, &key, &new_bytes);
                    batch.delete_cf(&cf_state, Self::state_index_key(machine, from_state, id));
                    batch.put_cf(&cf_state, Self::state_index_key(machine, to_state, id), b"");
                    count += 1;
                }
            }
        }

        self.db
            .write(batch)
            .map_err(|e| SmqlError::storage(e.to_string()))?;

        Ok(count)
    }

    async fn bulk_update_instances(
        &self,
        machine: &str,
        mutations: &[Mutation],
    ) -> SmqlResult<u64> {
        let ids = self.scan_machine_index(machine)?;

        let cf_inst = self.cf(CF_INSTANCES)?;

        let mut batch = WriteBatchWithTransaction::<false>::default();
        let mut count = 0u64;

        for id in &ids {
            let key = Self::instance_key(machine, id);
            if let Some(bytes) = self
                .db
                .get_cf(&cf_inst, &key)
                .map_err(|e| SmqlError::storage(e.to_string()))?
            {
                let mut inst = Self::deserialize_instance(&bytes)?;
                if inst.machine == machine {
                    Self::apply_mutations(&mut inst, mutations);
                    inst.version += 1;
                    let new_bytes = Self::serialize_instance(&inst)?;
                    batch.put_cf(&cf_inst, &key, &new_bytes);
                    count += 1;
                }
            }
        }

        self.db
            .write(batch)
            .map_err(|e| SmqlError::storage(e.to_string()))?;

        Ok(count)
    }

    async fn store_timer(&self, timer: &StoredTimer) -> SmqlResult<()> {
        let cf = self.cf(CF_TIMERS)?;
        let key = Self::make_key(&[timer.instance_id.as_bytes(), timer.from_state.as_bytes()]);
        let bytes = serde_json::to_vec(timer)
            .map_err(|e| SmqlError::storage(format!("Serialize timer: {}", e)))?;
        self.db
            .put_cf(&cf, &key, &bytes)
            .map_err(|e| SmqlError::storage(e.to_string()))?;
        Ok(())
    }

    async fn remove_timer(&self, instance_id: &str, state: &str) -> SmqlResult<()> {
        let cf = self.cf(CF_TIMERS)?;
        let key = Self::make_key(&[instance_id.as_bytes(), state.as_bytes()]);
        self.db
            .delete_cf(&cf, &key)
            .map_err(|e| SmqlError::storage(e.to_string()))?;
        Ok(())
    }

    async fn remove_all_timers(&self, instance_id: &str) -> SmqlResult<()> {
        let cf = self.cf(CF_TIMERS)?;
        let prefix = Self::prefix_key(&[instance_id.as_bytes()]);
        let pairs = self.scan_prefix(CF_TIMERS, &prefix)?;
        let mut batch = WriteBatchWithTransaction::<false>::default();
        for (key, _) in pairs {
            batch.delete_cf(&cf, &key);
        }
        self.db
            .write(batch)
            .map_err(|e| SmqlError::storage(e.to_string()))?;
        Ok(())
    }

    async fn load_all_timers(&self) -> SmqlResult<Vec<StoredTimer>> {
        let cf = self.cf(CF_TIMERS)?;
        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        let mut timers = Vec::new();
        for item in iter {
            let (_, value) = item.map_err(|e| SmqlError::storage(e.to_string()))?;
            let timer: StoredTimer = serde_json::from_slice(&value)
                .map_err(|e| SmqlError::storage(format!("Deserialize timer: {}", e)))?;
            timers.push(timer);
        }
        Ok(timers)
    }

    async fn store_idempotency(
        &self,
        key: &str,
        response: &[u8],
        expires_at: chrono::DateTime<Utc>,
    ) -> SmqlResult<bool> {
        let cf = self.cf(CF_IDEMPOTENCY)?;
        // Check if already exists and not expired
        if let Some(existing) = self
            .db
            .get_cf(&cf, key.as_bytes())
            .map_err(|e| SmqlError::storage(e.to_string()))?
        {
            // Deserialize to check expiry: stored as (response_bytes, expires_at_rfc3339)
            if let Ok(entry) =
                serde_json::from_slice::<(Vec<u8>, String)>(&existing)
            {
                if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(&entry.1) {
                    if exp > Utc::now() {
                        return Ok(false); // Key exists and not expired
                    }
                }
            }
        }
        let entry = (response.to_vec(), expires_at.to_rfc3339());
        let serialized = serde_json::to_vec(&entry)
            .map_err(|e| SmqlError::storage(format!("Serialize idempotency: {}", e)))?;
        self.db
            .put_cf(&cf, key.as_bytes(), &serialized)
            .map_err(|e| SmqlError::storage(e.to_string()))?;
        Ok(true)
    }

    async fn get_idempotency(&self, key: &str) -> SmqlResult<Option<Vec<u8>>> {
        let cf = self.cf(CF_IDEMPOTENCY)?;
        if let Some(raw) = self
            .db
            .get_cf(&cf, key.as_bytes())
            .map_err(|e| SmqlError::storage(e.to_string()))?
        {
            let entry: (Vec<u8>, String) = serde_json::from_slice(&raw)
                .map_err(|e| SmqlError::storage(format!("Deserialize idempotency: {}", e)))?;
            if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(&entry.1) {
                if exp > Utc::now() {
                    return Ok(Some(entry.0));
                }
            }
            // Expired — clean up
            self.db
                .delete_cf(&cf, key.as_bytes())
                .map_err(|e| SmqlError::storage(e.to_string()))?;
        }
        Ok(None)
    }

    async fn cleanup_expired_idempotency(&self) -> SmqlResult<usize> {
        let cf = self.cf(CF_IDEMPOTENCY)?;
        let now = Utc::now();
        let mut expired_keys = Vec::new();

        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        for item in iter {
            let (key, value) = item.map_err(|e| SmqlError::storage(e.to_string()))?;
            if let Ok(entry) = serde_json::from_slice::<(Vec<u8>, String)>(&value) {
                if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(&entry.1) {
                    if exp <= now {
                        expired_keys.push(key.to_vec());
                    }
                }
            }
        }

        let count = expired_keys.len();
        for key in &expired_keys {
            self.db
                .delete_cf(&cf, key)
                .map_err(|e| SmqlError::storage(e.to_string()))?;
        }
        Ok(count)
    }

    async fn claim_instance(
        &self,
        id: &InstanceId,
        agent_id: &str,
        expires_at: chrono::DateTime<Utc>,
    ) -> SmqlResult<()> {
        let id_str = id.as_str();
        let cf_instances = self.cf(CF_INSTANCES)?;

        // Find the instance by scanning the machine index
        let cf_id = self.cf(CF_ID_INDEX)?;
        let machine_key = self
            .db
            .get_cf(&cf_id, id_str.as_bytes())
            .map_err(|e| SmqlError::storage(e.to_string()))?
            .ok_or_else(|| SmqlError::not_found("Instance", &id_str))?;
        let machine =
            String::from_utf8(machine_key).map_err(|e| SmqlError::storage(e.to_string()))?;

        let key = Self::instance_key(&machine, &id_str);
        let raw = self
            .db
            .get_cf(&cf_instances, &key)
            .map_err(|e| SmqlError::storage(e.to_string()))?
            .ok_or_else(|| SmqlError::not_found("Instance", &id_str))?;
        let mut inst: Instance =
            serde_json::from_slice(&raw).map_err(|e| SmqlError::storage(e.to_string()))?;

        let now = Utc::now();
        if let Some(ref holder) = inst.claimed_by {
            if holder != agent_id {
                if let Some(exp) = inst.claim_expires_at {
                    if exp > now {
                        return Err(SmqlError::conflict(format!(
                            "Instance already claimed by '{}'",
                            holder
                        )));
                    }
                }
            }
        }

        inst.claimed_by = Some(agent_id.to_string());
        inst.claim_expires_at = Some(expires_at);
        inst.updated_at = now;

        let serialized =
            serde_json::to_vec(&inst).map_err(|e| SmqlError::storage(e.to_string()))?;
        self.db
            .put_cf(&cf_instances, &key, &serialized)
            .map_err(|e| SmqlError::storage(e.to_string()))?;
        Ok(())
    }

    async fn release_claim(&self, id: &InstanceId, agent_id: &str) -> SmqlResult<()> {
        let id_str = id.as_str();
        let cf_instances = self.cf(CF_INSTANCES)?;

        let cf_id = self.cf(CF_ID_INDEX)?;
        let machine_key = self
            .db
            .get_cf(&cf_id, id_str.as_bytes())
            .map_err(|e| SmqlError::storage(e.to_string()))?
            .ok_or_else(|| SmqlError::not_found("Instance", &id_str))?;
        let machine =
            String::from_utf8(machine_key).map_err(|e| SmqlError::storage(e.to_string()))?;

        let key = Self::instance_key(&machine, &id_str);
        let raw = self
            .db
            .get_cf(&cf_instances, &key)
            .map_err(|e| SmqlError::storage(e.to_string()))?
            .ok_or_else(|| SmqlError::not_found("Instance", &id_str))?;
        let mut inst: Instance =
            serde_json::from_slice(&raw).map_err(|e| SmqlError::storage(e.to_string()))?;

        if let Some(ref holder) = inst.claimed_by {
            if holder != agent_id {
                return Err(SmqlError::conflict(format!(
                    "Instance claimed by '{}', not '{}'",
                    holder, agent_id
                )));
            }
        }

        inst.claimed_by = None;
        inst.claim_expires_at = None;
        inst.updated_at = Utc::now();

        let serialized =
            serde_json::to_vec(&inst).map_err(|e| SmqlError::storage(e.to_string()))?;
        self.db
            .put_cf(&cf_instances, &key, &serialized)
            .map_err(|e| SmqlError::storage(e.to_string()))?;
        Ok(())
    }

    async fn find_and_claim(
        &self,
        machine: &str,
        filter: &Filter,
        agent_id: &str,
        expires_at: chrono::DateTime<Utc>,
    ) -> SmqlResult<Option<Instance>> {
        let now = Utc::now();
        let candidates = self.find_instances(machine, filter).await?;

        let cf_instances = self.cf(CF_INSTANCES)?;
        for candidate in candidates {
            let id_str = candidate.id.as_str();
            let key = Self::instance_key(machine, &id_str);
            let raw = self
                .db
                .get_cf(&cf_instances, &key)
                .map_err(|e| SmqlError::storage(e.to_string()))?
                .ok_or_else(|| SmqlError::not_found("Instance", &id_str))?;
            let mut inst: Instance =
                serde_json::from_slice(&raw).map_err(|e| SmqlError::storage(e.to_string()))?;

            // Skip if claimed by another agent and not expired
            if let Some(ref holder) = inst.claimed_by {
                if holder != agent_id {
                    if let Some(exp) = inst.claim_expires_at {
                        if exp > now {
                            continue;
                        }
                    }
                }
            }

            inst.claimed_by = Some(agent_id.to_string());
            inst.claim_expires_at = Some(expires_at);
            inst.updated_at = now;

            let serialized =
                serde_json::to_vec(&inst).map_err(|e| SmqlError::storage(e.to_string()))?;
            self.db
                .put_cf(&cf_instances, &key, &serialized)
                .map_err(|e| SmqlError::storage(e.to_string()))?;
            return Ok(Some(inst));
        }

        Ok(None)
    }

    async fn store_event(&self, event: &crate::instance::StoredEvent) -> SmqlResult<()> {
        let cf = self.cf(CF_EVENTS)?;
        let serialized =
            serde_json::to_vec(event).map_err(|e| SmqlError::storage(e.to_string()))?;
        // Key = ULID string (preserves time-ordering in byte sort)
        self.db
            .put_cf(&cf, event.id.as_bytes(), &serialized)
            .map_err(|e| SmqlError::storage(e.to_string()))?;
        Ok(())
    }

    async fn get_events_after(
        &self,
        after_id: Option<&str>,
        machine: Option<&str>,
        event_name: Option<&str>,
        limit: usize,
    ) -> SmqlResult<Vec<crate::instance::StoredEvent>> {
        let cf = self.cf(CF_EVENTS)?;
        let start_key_buf = after_id.map(|after| {
            let mut k = after.as_bytes().to_vec();
            k.push(0x01);
            k
        });
        let mode = match &start_key_buf {
            Some(key) => IteratorMode::From(key, rocksdb::Direction::Forward),
            None => IteratorMode::Start,
        };

        let iter = self.db.iterator_cf(&cf, mode);
        let mut results = Vec::new();

        for item in iter {
            if results.len() >= limit {
                break;
            }
            let (_key, value) = item.map_err(|e| SmqlError::storage(e.to_string()))?;
            let event: crate::instance::StoredEvent = serde_json::from_slice(&value)
                .map_err(|e| SmqlError::storage(format!("Deserialize event: {}", e)))?;

            if let Some(m) = machine {
                if event.machine != m {
                    continue;
                }
            }
            if let Some(name) = event_name {
                if event.event_name != name {
                    continue;
                }
            }
            results.push(event);
        }

        Ok(results)
    }

    async fn cleanup_events_before(
        &self,
        before: chrono::DateTime<Utc>,
    ) -> SmqlResult<usize> {
        let cf = self.cf(CF_EVENTS)?;
        let mut expired_keys = Vec::new();

        let iter = self.db.iterator_cf(&cf, IteratorMode::Start);
        for item in iter {
            let (key, value) = item.map_err(|e| SmqlError::storage(e.to_string()))?;
            if let Ok(event) = serde_json::from_slice::<crate::instance::StoredEvent>(&value) {
                if event.timestamp < before {
                    expired_keys.push(key.to_vec());
                } else {
                    break; // Events are time-ordered, stop once we pass the cutoff
                }
            }
        }

        let count = expired_keys.len();
        for key in &expired_keys {
            self.db
                .delete_cf(&cf, key)
                .map_err(|e| SmqlError::storage(e.to_string()))?;
        }
        Ok(count)
    }
}
