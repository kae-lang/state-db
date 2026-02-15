use async_trait::async_trait;
use smql_ast::SmqlResult;
use std::collections::HashMap;

use crate::instance::{Filter, Instance, InstanceId, Mutation, TrailEntry, TrailFilter};

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
    async fn query_trails(&self, machine: &str, filter: &TrailFilter) -> SmqlResult<Vec<TrailEntry>>;
}
