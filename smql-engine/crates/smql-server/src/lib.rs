// SMQL Server — HTTP/JSON API for SMQL Engine

#[cfg(feature = "auth")]
pub mod auth;
pub mod handlers;
pub mod metrics;
pub mod server;
pub mod websocket;

pub use server::SmqlServer;

#[cfg(test)]
mod tests;
