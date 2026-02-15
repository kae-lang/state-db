// SMQL Server — HTTP/JSON API for SMQL Engine

pub mod handlers;
pub mod metrics;
pub mod server;
pub mod websocket;

pub use server::SmqlServer;

#[cfg(test)]
mod tests;
