// SMQL Storage — Pluggable storage backends

pub mod instance;
pub mod memory;
#[cfg(feature = "rocksdb")]
pub mod rocksdb;
pub mod traits;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "rocksdb"))]
mod rocksdb_tests;

pub use instance::{
    Filter, FilterPredicate, Instance, InstanceId, Mutation, TrailEntry, TrailFilter,
};
pub use memory::MemoryStorage;
#[cfg(feature = "rocksdb")]
pub use rocksdb::RocksDBStorage;
pub use traits::Storage;
