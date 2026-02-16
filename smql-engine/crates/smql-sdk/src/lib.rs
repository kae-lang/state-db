//! SMQL SDK — ergonomic Rust client for the SMQL server.
//!
//! # Quick Start
//!
//! ```no_run
//! use smql_sdk::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = SmqlClient::new("http://localhost:4200")?;
//!     assert!(client.health().await?);
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod error;
pub mod find;
pub mod smql_fmt;
pub mod subscription;
pub mod typed;
pub mod types;

/// Re-exports of the most commonly used types.
pub mod prelude {
    pub use crate::client::SmqlClient;
    pub use crate::error::{SdkError, SdkResult};
    pub use crate::find::{AggregateBuilder, FindBuilder};
    pub use crate::subscription::{Subscription, SubscriptionHandle};
    pub use crate::typed::{SmqlMachine, SmqlState, TypedInstance};
    pub use crate::types::*;
}
