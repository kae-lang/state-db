use async_trait::async_trait;
use chrono::Utc;
use rocksdb::{
    ColumnFamilyDescriptor, DBWithThreadMode, IteratorMode, MultiThreaded, Options, ReadOptions,
    WriteBatchWithTransaction,
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

const SEP: u8 = 0x00;

/// RocksDB-backed persistent storage backend.
///
/// Uses 7 column families: instances, state_index, machine_index, trails,
/// parent_index, id_index, and timers. Atomic writes use WriteBatch.
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

        let cf_names = [
            CF_INSTANCES,
            CF_STATE_INDEX,
            CF_MACHINE_INDEX,
            CF_TRAILS,
            CF_PARENT_INDEX,
            CF_ID_INDEX,
            CF_TIMERS,
        ];

        let cf_descriptors: Vec<ColumnFamilyDescriptor> = cf_names
            .iter()
            .map(|name| ColumnFamilyDescriptor::new(*name, Options::default()))
            .collect();

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
        let cf_id = self.db.cf_handle(CF_ID_INDEX).unwrap();
        match self
            .db
            .get_cf(&cf_id, id.as_bytes())
            .map_err(|e| SmqlError::storage(e.to_string()))?
        {
            Some(machine_bytes) => {
                let machine = String::from_utf8(machine_bytes.to_vec())
                    .map_err(|e| SmqlError::storage(format!("Invalid UTF-8 in id_index: {}", e)))?;
                let cf_inst = self.db.cf_handle(CF_INSTANCES).unwrap();
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
        let cf = self.db.cf_handle(cf_name).unwrap();

        // Compute upper bound: increment last byte to get the exclusive end of prefix range.
        let upper_bound = Self::prefix_upper_bound(prefix);

        let mut read_opts = ReadOptions::default();
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

        let cf_inst = self.db.cf_handle(CF_INSTANCES).unwrap();
        let cf_state = self.db.cf_handle(CF_STATE_INDEX).unwrap();
        let cf_machine = self.db.cf_handle(CF_MACHINE_INDEX).unwrap();
        let cf_id = self.db.cf_handle(CF_ID_INDEX).unwrap();
        let cf_parent = self.db.cf_handle(CF_PARENT_INDEX).unwrap();

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

        let cf_inst = self.db.cf_handle(CF_INSTANCES).unwrap();
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

        let cf_inst = self.db.cf_handle(CF_INSTANCES).unwrap();
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

    async fn transition_instance(
        &self,
        id: &InstanceId,
        expected_version: u64,
        new_state: &str,
        mutations: &[Mutation],
        trail_entry: TrailEntry,
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

        let old_state = instance.state.clone();
        let machine = instance.machine.clone();

        Self::apply_mutations(&mut instance, mutations);
        instance.state = new_state.to_string();
        instance.state_entered_at = Utc::now();
        instance.version += 1;
        instance.trail_length += 1;

        let cf_inst = self.db.cf_handle(CF_INSTANCES).unwrap();
        let cf_state = self.db.cf_handle(CF_STATE_INDEX).unwrap();
        let cf_trails = self.db.cf_handle(CF_TRAILS).unwrap();

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

        Ok(())
    }

    async fn delete_instance(&self, id: &InstanceId) -> SmqlResult<()> {
        let id_str = id.as_str();
        let instance = self
            .load_instance(&id_str)?
            .ok_or_else(|| SmqlError::not_found("Instance", &id_str))?;

        let cf_inst = self.db.cf_handle(CF_INSTANCES).unwrap();
        let cf_state = self.db.cf_handle(CF_STATE_INDEX).unwrap();
        let cf_machine = self.db.cf_handle(CF_MACHINE_INDEX).unwrap();
        let cf_id = self.db.cf_handle(CF_ID_INDEX).unwrap();
        let cf_parent = self.db.cf_handle(CF_PARENT_INDEX).unwrap();
        let cf_trails = self.db.cf_handle(CF_TRAILS).unwrap();

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
        let cf_timers = self.db.cf_handle(CF_TIMERS).unwrap();
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
        let cf_trails = self.db.cf_handle(CF_TRAILS).unwrap();
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
        let cf_id = self.db.cf_handle(CF_ID_INDEX).unwrap();
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

        let cf_inst = self.db.cf_handle(CF_INSTANCES).unwrap();
        let cf_state = self.db.cf_handle(CF_STATE_INDEX).unwrap();

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

        let cf_inst = self.db.cf_handle(CF_INSTANCES).unwrap();

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
        let cf = self.db.cf_handle(CF_TIMERS).unwrap();
        let key = Self::make_key(&[timer.instance_id.as_bytes(), timer.from_state.as_bytes()]);
        let bytes = serde_json::to_vec(timer)
            .map_err(|e| SmqlError::storage(format!("Serialize timer: {}", e)))?;
        self.db
            .put_cf(&cf, &key, &bytes)
            .map_err(|e| SmqlError::storage(e.to_string()))?;
        Ok(())
    }

    async fn remove_timer(&self, instance_id: &str, state: &str) -> SmqlResult<()> {
        let cf = self.db.cf_handle(CF_TIMERS).unwrap();
        let key = Self::make_key(&[instance_id.as_bytes(), state.as_bytes()]);
        self.db
            .delete_cf(&cf, &key)
            .map_err(|e| SmqlError::storage(e.to_string()))?;
        Ok(())
    }

    async fn remove_all_timers(&self, instance_id: &str) -> SmqlResult<()> {
        let cf = self.db.cf_handle(CF_TIMERS).unwrap();
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
        let cf = self.db.cf_handle(CF_TIMERS).unwrap();
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
}
