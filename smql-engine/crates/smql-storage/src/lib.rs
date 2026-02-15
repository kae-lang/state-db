// SMQL Storage — Pluggable storage backends

pub mod instance;
pub mod memory;
pub mod traits;

#[cfg(test)]
mod tests;

pub use instance::{
    Filter, FilterPredicate, Instance, InstanceId, Mutation, TrailEntry, TrailFilter,
};
pub use memory::MemoryStorage;
pub use traits::Storage;
