// SMQL Catalog — Machine definitions, schema registry

mod validation;

#[cfg(test)]
mod tests;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use smql_ast::machine::MachineDefinition;
use smql_ast::{SmqlError, SmqlResult};
use std::sync::Arc;

pub use validation::{validate_machine, ValidationWarning};

/// In-memory registry of machine definitions.
/// Thread-safe via DashMap for concurrent access.
#[derive(Clone)]
pub struct MachineCatalog {
    machines: Arc<DashMap<String, MachineEntry>>,
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
        }
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

        self.machines
            .alter(&name, |_, mut entry| {
                entry.history.push(entry.definition.clone());
                entry.version += 1;
                entry.definition = definition.clone();
                entry
            });

        if !self.machines.contains_key(&name) {
            return Err(SmqlError::not_found("Machine", &name));
        }

        Ok(warnings)
    }

    /// Serialize the catalog to JSON for persistence.
    pub fn serialize(&self) -> SmqlResult<String> {
        let entries: Vec<MachineEntry> = self
            .machines
            .iter()
            .map(|e| e.value().clone())
            .collect();
        serde_json::to_string_pretty(&entries).map_err(|e| SmqlError::internal(e.to_string()))
    }

    /// Deserialize and load a catalog from JSON.
    pub fn deserialize(json: &str) -> SmqlResult<Self> {
        let entries: Vec<MachineEntry> =
            serde_json::from_str(json).map_err(|e| SmqlError::internal(e.to_string()))?;
        let catalog = Self::new();
        for entry in entries {
            catalog.machines.insert(entry.definition.name.clone(), entry);
        }
        Ok(catalog)
    }
}

impl Default for MachineCatalog {
    fn default() -> Self {
        Self::new()
    }
}
