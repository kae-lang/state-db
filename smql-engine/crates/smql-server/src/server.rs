use axum::Router;
use smql_catalog::MachineCatalog;
use smql_engine_core::Engine;
use smql_storage::MemoryStorage;
use std::sync::Arc;

use crate::handlers;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<Engine>,
}

/// The SMQL HTTP server.
pub struct SmqlServer {
    state: AppState,
}

impl SmqlServer {
    /// Create a new server with in-memory storage.
    pub fn new() -> Self {
        let catalog = Arc::new(MachineCatalog::new());
        let storage = Arc::new(MemoryStorage::new());
        let engine = Arc::new(Engine::new(catalog, storage));

        Self {
            state: AppState { engine },
        }
    }

    /// Create a server with an existing engine (for testing).
    pub fn with_engine(engine: Arc<Engine>) -> Self {
        Self {
            state: AppState { engine },
        }
    }

    /// Build the axum Router.
    pub fn router(&self) -> Router {
        handlers::build_router(self.state.clone())
    }

    /// Start the server on the given address.
    pub async fn serve(self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let app = self.router();
        let listener = tokio::net::TcpListener::bind(addr).await?;
        tracing::info!("SMQL server listening on {}", addr);
        axum::serve(listener, app).await?;
        Ok(())
    }
}

impl Default for SmqlServer {
    fn default() -> Self {
        Self::new()
    }
}
