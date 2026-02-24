use async_trait::async_trait;
use smql_ast::SmqlResult;
use std::collections::HashMap;

use crate::instance::{
    Filter, Instance, InstanceId, Mutation, StoredTimer, TrailEntry, TrailFilter,
};

/// Pluggable storage backend trait.
/// All storage implementations must be Send + Sync for concurrent access.
#[async_trait]
pub trait Storage: Send + Sync {
    // --- Instance operations ---

    /// Store a new instance. Returns error if ID already exists.
    async fn store_instance(&self, instance: &Instance) -> SmqlResult<()>;

    /// Retrieve an instance by ID.
    async fn get_instance(&self, id: &InstanceId) -> SmqlResult<Option<Instance>>;

    /// Find instances matching a filter within a specific machine type.
    async fn find_instances(&self, machine: &str, filter: &Filter) -> SmqlResult<Vec<Instance>>;

    /// Update an instance's data fields via mutations.
    /// Uses optimistic concurrency: fails if the version doesn't match.
    async fn update_instance(
        &self,
        id: &InstanceId,
        expected_version: u64,
        mutations: &[Mutation],
    ) -> SmqlResult<()>;

    /// Transition an instance to a new state atomically.
    /// Updates state, state_entered_at, version, and appends a trail entry.
    async fn transition_instance(
        &self,
        id: &InstanceId,
        expected_version: u64,
        new_state: &str,
        mutations: &[Mutation],
        trail_entry: TrailEntry,
    ) -> SmqlResult<()>;

    /// Delete an instance.
    async fn delete_instance(&self, id: &InstanceId) -> SmqlResult<()>;

    /// Count instances by state for a given machine type.
    async fn count_by_state(&self, machine: &str) -> SmqlResult<HashMap<String, usize>>;

    // --- Trail operations ---

    /// Append an entry to the transition trail.
    async fn append_trail_entry(&self, entry: &TrailEntry) -> SmqlResult<()>;

    /// Get the full trail for an instance.
    async fn get_trail(&self, id: &InstanceId) -> SmqlResult<Vec<TrailEntry>>;

    /// Query trail entries across instances of a machine.
    async fn query_trails(
        &self,
        machine: &str,
        filter: &TrailFilter,
    ) -> SmqlResult<Vec<TrailEntry>>;

    // --- Parent-child composition operations ---

    /// Find child instances of a parent, optionally filtered by child machine type.
    async fn find_children(
        &self,
        parent_id: &InstanceId,
        child_machine: Option<&str>,
    ) -> SmqlResult<Vec<Instance>>;

    /// Get the parent instance of a child (reads child's parent_id, then fetches parent).
    async fn get_parent(&self, child_id: &InstanceId) -> SmqlResult<Option<Instance>>;

    // --- Schema migration operations ---

    /// Migrate all instances of a machine from one state to another.
    /// Updates state, state_entered_at, version, and state indices.
    /// Returns the number of migrated instances.
    async fn migrate_instances_state(
        &self,
        machine: &str,
        from_state: &str,
        to_state: &str,
    ) -> SmqlResult<u64>;

    /// Apply mutations to all instances of a machine.
    /// Skips version checks (schema migration operation).
    /// Returns the number of updated instances.
    async fn bulk_update_instances(&self, machine: &str, mutations: &[Mutation])
        -> SmqlResult<u64>;

    // --- Timer persistence ---

    /// Store a timer. Key = "{instance_id}:{state}". Overwrites existing.
    async fn store_timer(&self, timer: &StoredTimer) -> SmqlResult<()>;

    /// Remove a timer for a specific instance and state.
    async fn remove_timer(&self, instance_id: &str, state: &str) -> SmqlResult<()>;

    /// Remove all timers for a specific instance.
    async fn remove_all_timers(&self, instance_id: &str) -> SmqlResult<()>;

    /// Load all stored timers (for restore on startup).
    async fn load_all_timers(&self) -> SmqlResult<Vec<StoredTimer>>;

    // --- Idempotency key operations ---

    /// Store an idempotency entry. Returns `true` if newly stored (key was not present),
    /// `false` if the key already existed (duplicate).
    async fn store_idempotency(
        &self,
        key: &str,
        response: &[u8],
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> SmqlResult<bool>;

    /// Retrieve a cached response for an idempotency key.
    async fn get_idempotency(&self, key: &str) -> SmqlResult<Option<Vec<u8>>>;

    /// Remove expired idempotency entries. Returns the number removed.
    async fn cleanup_expired_idempotency(&self) -> SmqlResult<usize>;

    // --- Instance claiming operations ---

    /// Atomically claim an instance. Sets claimed_by and claim_expires_at.
    /// Fails if already claimed by a different agent (and claim not expired).
    async fn claim_instance(
        &self,
        id: &InstanceId,
        agent_id: &str,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> SmqlResult<()>;

    /// Release a claim on an instance. Only succeeds if claimed_by matches agent_id.
    async fn release_claim(&self, id: &InstanceId, agent_id: &str) -> SmqlResult<()>;

    /// Find an unclaimed instance matching the filter and atomically claim it.
    /// Returns None if no unclaimed instance matches.
    async fn find_and_claim(
        &self,
        machine: &str,
        filter: &Filter,
        agent_id: &str,
        expires_at: chrono::DateTime<chrono::Utc>,
    ) -> SmqlResult<Option<Instance>>;

    // --- Durable event log operations ---

    /// Store an event entry in the durable event log.
    async fn store_event(&self, event: &crate::instance::StoredEvent) -> SmqlResult<()>;

    /// Retrieve events after a given event ID (or from the beginning if None).
    /// Optionally filter by machine and/or event name.
    async fn get_events_after(
        &self,
        after_id: Option<&str>,
        machine: Option<&str>,
        event_name: Option<&str>,
        limit: usize,
    ) -> SmqlResult<Vec<crate::instance::StoredEvent>>;

    /// Remove events older than the given timestamp. Returns the number removed.
    async fn cleanup_events_before(
        &self,
        before: chrono::DateTime<chrono::Utc>,
    ) -> SmqlResult<usize>;
}
