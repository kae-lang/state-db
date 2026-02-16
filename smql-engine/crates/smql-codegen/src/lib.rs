//! SMQL Codegen — generates typed Rust code from `.smql` machine definitions.

pub mod error;
pub mod generator;
pub mod rust_gen;
pub mod type_map;

pub use error::CodegenError;
pub use generator::{CodeGenerator, GeneratedFile};
