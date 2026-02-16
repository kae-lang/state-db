use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use smql_ast::{SmqlError, SmqlResult};
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use crate::instance::{Filter, Instance, InstanceId, Mutation, StoredTimer, TrailEntry, TrailFilter};
use crate::traits::Storage;

/// In-memory storage backend using DashMap for concurrent access.
/// Suitable for development, testing, and single-node deployments.
pub struct MemoryStorage {
    /// Primary instance storage: id -> Instance
    instances: DashMap<String, Instance>,
    /// State index: "{machine}:{state}" -> set of instance IDs
    state_index: DashMap<String, HashSet<String>>,
    /// Machine index: machine -> set of instance IDs
    machine_index: DashMap<String, HashSet<String>>,
    /// Trail storage: instance_id -> ordered trail entries
    trails: DashMap<String, RwLock<Vec<TrailEntry>>>,
    /// Parent-child index: parent_id -> set of child instance IDs
    parent_index: DashMap<String, HashSet<String>>,
    /// Timer persistence: "{instance_id}:{state}" -> StoredTimer
    timers: DashMap<String, StoredTimer>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            instances: DashMap::new(),
            state_index: DashMap::new(),
            machine_index: DashMap::new(),
            trails: DashMap::new(),
            parent_index: DashMap::new(),
            timers: DashMap::new(),
        }
    }

    fn state_index_key(machine: &str, state: &str) -> String {
        format!("{}:{}", machine, state)
    }

    fn add_to_state_index(&self, machine: &str, state: &str, id: &str) {
        let key = Self::state_index_key(machine, state);
        self.state_index
            .entry(key)
            .or_default()
            .insert(id.to_string());
    }

    fn remove_from_state_index(&self, machine: &str, state: &str, id: &str) {
        let key = Self::state_index_key(machine, state);
        if let Some(mut set) = self.state_index.get_mut(&key) {
            set.remove(id);
        }
    }

    fn add_to_machine_index(&self, machine: &str, id: &str) {
        self.machine_index
            .entry(machine.to_string())
            .or_default()
            .insert(id.to_string());
    }

    fn remove_from_machine_index(&self, machine: &str, id: &str) {
        if let Some(mut set) = self.machine_index.get_mut(machine) {
            set.remove(id);
        }
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
                    if let Some(smql_ast::value::Value::Int(v)) = instance.data.get_mut(field) {
                        *v += delta;
                    }
                }
                Mutation::AppendToList(field, value) => {
                    if let Some(smql_ast::value::Value::List(list)) = instance.data.get_mut(field) {
                        list.push(value.clone());
                    }
                }
            }
        }
        instance.updated_at = Utc::now();
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Storage for MemoryStorage {
    async fn store_instance(&self, instance: &Instance) -> SmqlResult<()> {
        let id_str = instance.id.as_str();

        if self.instances.contains_key(&id_str) {
            return Err(SmqlError::Conflict {
                message: format!("Instance '{}' already exists", id_str),
                hint: None,
            });
        }

        self.instances.insert(id_str.clone(), instance.clone());
        self.add_to_state_index(&instance.machine, &instance.state, &id_str);
        self.add_to_machine_index(&instance.machine, &id_str);
        self.trails.insert(id_str.clone(), RwLock::new(Vec::new()));

        // Update parent-child index if this is a child instance
        if let Some(parent_id) = &instance.parent_id {
            self.parent_index
                .entry(parent_id.as_str())
                .or_default()
                .insert(id_str);
        }

        Ok(())
    }

    async fn get_instance(&self, id: &InstanceId) -> SmqlResult<Option<Instance>> {
        let id_str = id.as_str();
        Ok(self.instances.get(&id_str).map(|entry| entry.clone()))
    }

    async fn find_instances(&self, machine: &str, filter: &Filter) -> SmqlResult<Vec<Instance>> {
        // Use state index if filtering by specific state
        let candidate_ids: Vec<String> = if let Some(state) = &filter.state {
            let key = Self::state_index_key(machine, state);
            self.state_index
                .get(&key)
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default()
        } else if let Some(states) = &filter.states {
            let mut ids = Vec::new();
            for state in states {
                let key = Self::state_index_key(machine, state);
                if let Some(set) = self.state_index.get(&key) {
                    ids.extend(set.iter().cloned());
                }
            }
            ids
        } else {
            // Full scan of machine index
            self.machine_index
                .get(machine)
                .map(|set| set.iter().cloned().collect())
                .unwrap_or_default()
        };

        let mut results: Vec<Instance> = candidate_ids
            .iter()
            .filter_map(|id| self.instances.get(id).map(|entry| entry.clone()))
            .filter(|inst| inst.machine == machine)
            .filter(|inst| {
                if let Some(pred) = &filter.predicate {
                    pred.matches(&inst.data)
                } else {
                    true
                }
            })
            .collect();

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

        let mut entry = self
            .instances
            .get_mut(&id_str)
            .ok_or_else(|| SmqlError::not_found("Instance", &id_str))?;

        if entry.version != expected_version {
            return Err(SmqlError::Conflict {
                message: format!(
                    "Version conflict for instance '{}': expected {}, got {}",
                    id_str, expected_version, entry.version
                ),
                hint: Some("Re-read the instance and retry".into()),
            });
        }

        Self::apply_mutations(&mut entry, mutations);
        entry.version += 1;

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

        let mut entry = self
            .instances
            .get_mut(&id_str)
            .ok_or_else(|| SmqlError::not_found("Instance", &id_str))?;

        if entry.version != expected_version {
            return Err(SmqlError::Conflict {
                message: format!(
                    "Version conflict for instance '{}': expected {}, got {}",
                    id_str, expected_version, entry.version
                ),
                hint: Some("Re-read the instance and retry".into()),
            });
        }

        let old_state = entry.state.clone();
        let machine = entry.machine.clone();

        // Apply mutations
        Self::apply_mutations(&mut entry, mutations);

        // Update state
        entry.state = new_state.to_string();
        entry.state_entered_at = Utc::now();
        entry.version += 1;
        entry.trail_length += 1;

        // Drop the mutable reference before updating indices
        drop(entry);

        // Update state index
        self.remove_from_state_index(&machine, &old_state, &id_str);
        self.add_to_state_index(&machine, new_state, &id_str);

        // Append trail entry
        if let Some(trail) = self.trails.get(&id_str) {
            if let Ok(mut entries) = trail.write() {
                entries.push(trail_entry);
            }
        }

        Ok(())
    }

    async fn delete_instance(&self, id: &InstanceId) -> SmqlResult<()> {
        let id_str = id.as_str();

        let (_, instance) = self
            .instances
            .remove(&id_str)
            .ok_or_else(|| SmqlError::not_found("Instance", &id_str))?;

        self.remove_from_state_index(&instance.machine, &instance.state, &id_str);
        self.remove_from_machine_index(&instance.machine, &id_str);
        self.trails.remove(&id_str);

        // Remove from parent-child index if this is a child
        if let Some(parent_id) = &instance.parent_id {
            if let Some(mut children) = self.parent_index.get_mut(&parent_id.as_str()) {
                children.remove(&id_str);
            }
        }

        Ok(())
    }

    async fn count_by_state(&self, machine: &str) -> SmqlResult<HashMap<String, usize>> {
        let mut counts: HashMap<String, usize> = HashMap::new();

        if let Some(ids) = self.machine_index.get(machine) {
            for id in ids.iter() {
                if let Some(inst) = self.instances.get(id) {
                    *counts.entry(inst.state.clone()).or_insert(0) += 1;
                }
            }
        }

        Ok(counts)
    }

    async fn append_trail_entry(&self, entry: &TrailEntry) -> SmqlResult<()> {
        let id_str = entry.instance_id.as_str();

        let trail = self
            .trails
            .entry(id_str)
            .or_insert_with(|| RwLock::new(Vec::new()));

        trail
            .write()
            .map_err(|e| SmqlError::internal(format!("Trail lock poisoned: {}", e)))?
            .push(entry.clone());

        Ok(())
    }

    async fn get_trail(&self, id: &InstanceId) -> SmqlResult<Vec<TrailEntry>> {
        let id_str = id.as_str();

        let trail = self
            .trails
            .get(&id_str)
            .ok_or_else(|| SmqlError::not_found("Trail", &id_str))?;

        trail
            .read()
            .map(|entries| entries.clone())
            .map_err(|e| SmqlError::internal(format!("Trail lock poisoned: {}", e)))
    }

    async fn query_trails(&self, machine: &str, filter: &TrailFilter) -> SmqlResult<Vec<TrailEntry>> {
        let mut results = Vec::new();

        let ids: Vec<String> = self
            .machine_index
            .get(machine)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();

        for id in &ids {
            if let Some(trail) = self.trails.get(id) {
                if let Ok(entries) = trail.read() {
                    for entry in entries.iter() {
                        let matches = filter.from_state.as_ref().map_or(true, |s| entry.from_state == *s)
                            && filter.to_state.as_ref().map_or(true, |s| entry.to_state == *s)
                            && filter.actor.as_ref().map_or(true, |a| entry.actor.as_ref() == Some(a))
                            && filter.after.map_or(true, |t| entry.timestamp > t)
                            && filter.before.map_or(true, |t| entry.timestamp < t);

                        if matches {
                            results.push(entry.clone());
                        }
                    }
                }
            }
        }

        // Sort by timestamp
        results.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        if let Some(limit) = filter.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    async fn find_children(&self, parent_id: &InstanceId, child_machine: Option<&str>) -> SmqlResult<Vec<Instance>> {
        let parent_str = parent_id.as_str();
        let child_ids: Vec<String> = self
            .parent_index
            .get(&parent_str)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();

        let mut results = Vec::new();
        for child_id in &child_ids {
            if let Some(inst) = self.instances.get(child_id) {
                if let Some(machine) = child_machine {
                    if inst.machine == machine {
                        results.push(inst.clone());
                    }
                } else {
                    results.push(inst.clone());
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
        let key = Self::state_index_key(machine, from_state);
        let ids: Vec<String> = self
            .state_index
            .get(&key)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();

        let mut count = 0u64;
        for id in &ids {
            if let Some(mut entry) = self.instances.get_mut(id) {
                if entry.machine == machine && entry.state == from_state {
                    entry.state = to_state.to_string();
                    entry.state_entered_at = Utc::now();
                    entry.updated_at = Utc::now();
                    entry.version += 1;
                    count += 1;
                }
            }
        }

        // Update state indices
        for id in &ids {
            self.remove_from_state_index(machine, from_state, id);
            self.add_to_state_index(machine, to_state, id);
        }

        Ok(count)
    }

    async fn bulk_update_instances(
        &self,
        machine: &str,
        mutations: &[Mutation],
    ) -> SmqlResult<u64> {
        let ids: Vec<String> = self
            .machine_index
            .get(machine)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default();

        let mut count = 0u64;
        for id in &ids {
            if let Some(mut entry) = self.instances.get_mut(id) {
                if entry.machine == machine {
                    Self::apply_mutations(&mut entry, mutations);
                    entry.version += 1;
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    async fn store_timer(&self, timer: &StoredTimer) -> SmqlResult<()> {
        let key = format!("{}:{}", timer.instance_id, timer.from_state);
        self.timers.insert(key, timer.clone());
        Ok(())
    }

    async fn remove_timer(&self, instance_id: &str, state: &str) -> SmqlResult<()> {
        let key = format!("{}:{}", instance_id, state);
        self.timers.remove(&key);
        Ok(())
    }

    async fn remove_all_timers(&self, instance_id: &str) -> SmqlResult<()> {
        let prefix = format!("{}:", instance_id);
        let keys: Vec<String> = self
            .timers
            .iter()
            .filter(|entry| entry.key().starts_with(&prefix))
            .map(|entry| entry.key().clone())
            .collect();
        for key in keys {
            self.timers.remove(&key);
        }
        Ok(())
    }

    async fn load_all_timers(&self) -> SmqlResult<Vec<StoredTimer>> {
        Ok(self.timers.iter().map(|entry| entry.value().clone()).collect())
    }
}
