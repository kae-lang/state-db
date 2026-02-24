// SMQL Catalog — Machine definitions, schema registry

mod validation;

#[cfg(test)]
mod tests;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use smql_ast::machine::{MachineDefinition, PolicyDefinition};
use smql_ast::rule::RuleDefinition;
use smql_ast::saga::SagaDefinition;
use smql_ast::subscription::SubscriptionDefinition;
use smql_ast::view::{ProjectionDefinition, ViewDefinition};
use smql_ast::{SmqlError, SmqlResult};
use std::sync::Arc;

pub use validation::{validate_machine, ValidationWarning};

/// In-memory registry of machine definitions.
/// Thread-safe via DashMap for concurrent access.
#[derive(Clone)]
pub struct MachineCatalog {
    machines: Arc<DashMap<String, MachineEntry>>,
    policies: Arc<DashMap<String, PolicyDefinition>>,
    views: Arc<DashMap<String, ViewDefinition>>,
    projections: Arc<DashMap<String, ProjectionDefinition>>,
    rules: Arc<DashMap<String, RuleDefinition>>,
    subscriptions: Arc<DashMap<String, SubscriptionDefinition>>,
    sagas: Arc<DashMap<String, SagaDefinition>>,
}

/// A versioned machine entry in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineEntry {
    pub definition: MachineDefinition,
    pub version: u64,
    pub history: Vec<MachineDefinition>,
}

impl MachineCatalog {
    pub fn new() -> Self {
        Self {
            machines: Arc::new(DashMap::new()),
            policies: Arc::new(DashMap::new()),
            views: Arc::new(DashMap::new()),
            projections: Arc::new(DashMap::new()),
            rules: Arc::new(DashMap::new()),
            subscriptions: Arc::new(DashMap::new()),
            sagas: Arc::new(DashMap::new()),
        }
    }

    /// Register a named saga.
    pub fn register_saga(&self, saga: SagaDefinition) {
        self.sagas.insert(saga.name.clone(), saga);
    }

    /// Retrieve a saga by name.
    pub fn get_saga(&self, name: &str) -> SmqlResult<SagaDefinition> {
        self.sagas
            .get(name)
            .map(|s| s.value().clone())
            .ok_or_else(|| SmqlError::not_found("Saga", name))
    }

    /// List all saga names.
    pub fn list_sagas(&self) -> Vec<String> {
        self.sagas.iter().map(|e| e.key().clone()).collect()
    }

    /// List sagas triggered by ON ENTER on a given machine+state.
    pub fn sagas_for_enter(&self, machine: &str, state: &str) -> Vec<SagaDefinition> {
        use smql_ast::saga::SagaTrigger;
        self.sagas
            .iter()
            .filter(|e| matches!(
                &e.value().trigger,
                SagaTrigger::OnEnter { machine: m, state: s } if m == machine && s == state
            ))
            .map(|e| e.value().clone())
            .collect()
    }

    /// List sagas triggered by ON SPAWN on a given machine.
    pub fn sagas_for_spawn(&self, machine: &str) -> Vec<SagaDefinition> {
        use smql_ast::saga::SagaTrigger;
        self.sagas
            .iter()
            .filter(|e| matches!(
                &e.value().trigger,
                SagaTrigger::OnSpawn { machine: m } if m == machine
            ))
            .map(|e| e.value().clone())
            .collect()
    }

    /// Register a named subscription.
    pub fn register_subscription(&self, sub: SubscriptionDefinition) {
        self.subscriptions.insert(sub.name.clone(), sub);
    }

    /// Retrieve a subscription by name.
    pub fn get_subscription(&self, name: &str) -> SmqlResult<SubscriptionDefinition> {
        self.subscriptions
            .get(name)
            .map(|s| s.value().clone())
            .ok_or_else(|| SmqlError::not_found("Subscription", name))
    }

    /// List all subscriptions that match a given machine + trigger type.
    pub fn subscriptions_for_machine(&self, machine: &str) -> Vec<SubscriptionDefinition> {
        use smql_ast::subscription::SubscriptionEvent;
        self.subscriptions
            .iter()
            .filter(|e| matches!(
                &e.value().event,
                SubscriptionEvent::OnEnter { machine: m, .. }
                | SubscriptionEvent::OnExit { machine: m, .. }
                | SubscriptionEvent::OnSpawn { machine: m }
                | SubscriptionEvent::OnTransition { machine: m, .. }
                if m == machine
            ))
            .map(|e| e.value().clone())
            .collect()
    }

    /// List all subscription names.
    pub fn list_subscriptions(&self) -> Vec<String> {
        self.subscriptions.iter().map(|e| e.key().clone()).collect()
    }

    /// Register a named rule (cross-instance invariant).
    pub fn register_rule(&self, rule: RuleDefinition) {
        self.rules.insert(rule.name.clone(), rule);
    }

    /// Retrieve a rule by name.
    pub fn get_rule(&self, name: &str) -> SmqlResult<RuleDefinition> {
        self.rules
            .get(name)
            .map(|r| r.value().clone())
            .ok_or_else(|| SmqlError::not_found("Rule", name))
    }

    /// List all rules that apply to a given machine (BeforeTransition or BeforeAnyTransition).
    pub fn rules_for_machine(&self, machine: &str) -> Vec<RuleDefinition> {
        use smql_ast::rule::RuleTrigger;
        self.rules
            .iter()
            .filter(|e| matches!(
                &e.value().trigger,
                RuleTrigger::BeforeTransition { machine: m } if m == machine
            ) || matches!(&e.value().trigger, RuleTrigger::BeforeAnyTransition))
            .map(|e| e.value().clone())
            .collect()
    }

    /// List all rule names.
    pub fn list_rules(&self) -> Vec<String> {
        self.rules.iter().map(|e| e.key().clone()).collect()
    }

    /// Register a named view.
    pub fn register_view(&self, view: ViewDefinition) {
        self.views.insert(view.name.clone(), view);
    }

    /// Retrieve a view by name.
    pub fn get_view(&self, name: &str) -> SmqlResult<ViewDefinition> {
        self.views
            .get(name)
            .map(|v| v.value().clone())
            .ok_or_else(|| SmqlError::not_found("View", name))
    }

    /// List all view names.
    pub fn list_views(&self) -> Vec<String> {
        self.views.iter().map(|e| e.key().clone()).collect()
    }

    /// Register a named projection.
    pub fn register_projection(&self, projection: ProjectionDefinition) {
        self.projections.insert(projection.name.clone(), projection);
    }

    /// Retrieve a projection definition by name.
    pub fn get_projection_def(&self, name: &str) -> SmqlResult<ProjectionDefinition> {
        self.projections
            .get(name)
            .map(|p| p.value().clone())
            .ok_or_else(|| SmqlError::not_found("Projection", name))
    }

    /// List all projection names.
    pub fn list_projections(&self) -> Vec<String> {
        self.projections.iter().map(|e| e.key().clone()).collect()
    }

    /// Register a named policy (reusable guard bundle).
    pub fn register_policy(&self, policy: PolicyDefinition) {
        self.policies.insert(policy.name.clone(), policy);
    }

    /// Retrieve a policy by name.
    pub fn get_policy(&self, name: &str) -> SmqlResult<PolicyDefinition> {
        self.policies
            .get(name)
            .map(|p| p.value().clone())
            .ok_or_else(|| SmqlError::not_found("Policy", name))
    }

    /// Check if a policy exists.
    pub fn has_policy(&self, name: &str) -> bool {
        self.policies.contains_key(name)
    }

    /// List all registered policy names.
    pub fn list_policies(&self) -> Vec<String> {
        self.policies.iter().map(|e| e.key().clone()).collect()
    }

    /// Register a new machine definition.
    /// Validates the definition before storing it.
    /// Returns any warnings from validation.
    pub fn register(&self, definition: MachineDefinition) -> SmqlResult<Vec<ValidationWarning>> {
        let warnings = validate_machine(&definition, self)?;

        let name = definition.name.clone();
        let version = definition.version;
        let entry = MachineEntry {
            definition,
            version,
            history: Vec::new(),
        };

        self.machines.insert(name, entry);
        Ok(warnings)
    }

    /// Register without validation (for bootstrapping / testing).
    pub fn register_unchecked(&self, definition: MachineDefinition) {
        let name = definition.name.clone();
        let version = definition.version;
        self.machines.insert(
            name,
            MachineEntry {
                definition,
                version,
                history: Vec::new(),
            },
        );
    }

    /// Retrieve a machine definition by name.
    pub fn get(&self, name: &str) -> SmqlResult<MachineDefinition> {
        self.machines
            .get(name)
            .map(|entry| entry.definition.clone())
            .ok_or_else(|| SmqlError::not_found("Machine", name))
    }

    /// Check if a machine exists.
    pub fn contains(&self, name: &str) -> bool {
        self.machines.contains_key(name)
    }

    /// Unregister a machine.
    pub fn unregister(&self, name: &str) -> SmqlResult<MachineDefinition> {
        self.machines
            .remove(name)
            .map(|(_, entry)| entry.definition)
            .ok_or_else(|| SmqlError::not_found("Machine", name))
    }

    /// List all machine names.
    pub fn list(&self) -> Vec<String> {
        self.machines.iter().map(|e| e.key().clone()).collect()
    }

    /// Get the current version of a machine.
    pub fn version(&self, name: &str) -> SmqlResult<u64> {
        self.machines
            .get(name)
            .map(|entry| entry.version)
            .ok_or_else(|| SmqlError::not_found("Machine", name))
    }

    /// Update a machine definition (increments version, preserves history).
    pub fn update(&self, definition: MachineDefinition) -> SmqlResult<Vec<ValidationWarning>> {
        let name = definition.name.clone();

        let warnings = validate_machine(&definition, self)?;

        // Use get_mut for atomic check-and-update (no TOCTOU race)
        match self.machines.get_mut(&name) {
            Some(mut entry) => {
                let old_def = entry.definition.clone();
                entry.history.push(old_def);
                entry.version += 1;
                entry.definition = definition;
            }
            None => {
                return Err(SmqlError::not_found("Machine", &name));
            }
        }

        Ok(warnings)
    }

    /// Serialize the catalog to JSON for persistence.
    pub fn serialize(&self) -> SmqlResult<String> {
        let entries: Vec<MachineEntry> = self.machines.iter().map(|e| e.value().clone()).collect();
        serde_json::to_string_pretty(&entries).map_err(|e| SmqlError::internal(e.to_string()))
    }

    /// Deserialize and load a catalog from JSON.
    pub fn deserialize(json: &str) -> SmqlResult<Self> {
        let entries: Vec<MachineEntry> =
            serde_json::from_str(json).map_err(|e| SmqlError::internal(e.to_string()))?;
        let catalog = Self::new();
        for entry in entries {
            catalog
                .machines
                .insert(entry.definition.name.clone(), entry);
        }
        Ok(catalog)
    }
}

impl Default for MachineCatalog {
    fn default() -> Self {
        Self::new()
    }
}
