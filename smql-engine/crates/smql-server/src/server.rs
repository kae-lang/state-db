use axum::Router;
use smql_catalog::MachineCatalog;
use smql_engine_core::Engine;
use smql_hooks::{EventBus, WebhookClient, WebhookConfig};
use smql_storage::{MemoryStorage, Storage};
use std::sync::Arc;

use crate::handlers;
use crate::metrics::SmqlMetrics;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<Engine>,
    pub metrics: Arc<SmqlMetrics>,
    pub event_bus: Arc<EventBus>,
}

/// The SMQL HTTP server.
pub struct SmqlServer {
    state: AppState,
    #[cfg(feature = "auth")]
    auth_config: Option<crate::auth::AuthConfig>,
}

impl SmqlServer {
    /// Create a new server with in-memory storage.
    pub fn new() -> Self {
        let catalog = Arc::new(MachineCatalog::new());
        let storage = Arc::new(MemoryStorage::new());
        let engine = Arc::new(Engine::new(catalog, storage));
        engine.wire_callback();
        let event_bus = engine.event_bus().clone();
        let metrics = Arc::new(SmqlMetrics::new());

        Self {
            state: AppState {
                engine,
                metrics,
                event_bus,
            },
            #[cfg(feature = "auth")]
            auth_config: None,
        }
    }

    /// Create a new server with a custom storage backend.
    pub fn with_storage(storage: Arc<dyn Storage>) -> Self {
        let catalog = Arc::new(MachineCatalog::new());
        let engine = Arc::new(Engine::new(catalog, storage));
        engine.wire_callback();
        let event_bus = engine.event_bus().clone();
        let metrics = Arc::new(SmqlMetrics::new());

        Self {
            state: AppState {
                engine,
                metrics,
                event_bus,
            },
            #[cfg(feature = "auth")]
            auth_config: None,
        }
    }

    /// Create a server with an existing engine (for testing).
    pub fn with_engine(engine: Arc<Engine>) -> Self {
        engine.wire_callback();
        let event_bus = engine.event_bus().clone();
        let metrics = Arc::new(SmqlMetrics::new());

        Self {
            state: AppState {
                engine,
                metrics,
                event_bus,
            },
            #[cfg(feature = "auth")]
            auth_config: None,
        }
    }

    /// Enable JWT authentication with the given config.
    #[cfg(feature = "auth")]
    pub fn with_auth(mut self, config: crate::auth::AuthConfig) -> Self {
        self.auth_config = Some(config);
        self
    }

    /// Build the axum Router.
    pub fn router(&self) -> Router {
        let router = handlers::build_router(self.state.clone());

        #[cfg(feature = "auth")]
        let router = if let Some(config) = &self.auth_config {
            router
                .layer(axum::middleware::from_fn(crate::auth::auth_middleware))
                .layer(crate::auth::AuthConfigLayer {
                    config: config.clone(),
                })
        } else {
            router
        };

        router
    }

    /// Start the server on the given address.
    pub async fn serve(self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Wire up webhook client for real HTTP POST
        let webhook_client = Arc::new(WebhookClient::new(WebhookConfig::default()));
        self.state
            .engine
            .hook_executor
            .set_webhook_client(webhook_client);

        // Restore persisted timers before starting the timer loop
        match self.state.engine.restore_timers().await {
            Ok(count) => {
                if count > 0 {
                    tracing::info!(count, "restored persisted timers");
                }
            }
            Err(e) => {
                tracing::warn!("failed to restore timers: {}", e);
            }
        }

        // Start background EventBus listener for timeout metrics
        crate::handlers::start_event_metrics_listener(
            self.state.event_bus.clone(),
            self.state.metrics.clone(),
        );

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

#[cfg(all(test, feature = "auth"))]
mod auth_integration_tests {
    use super::*;
    use crate::auth::{AuthClaims, AuthConfig};
    use axum::body::Body;
    use http::Request;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use tower::ServiceExt;

    fn test_auth_config() -> AuthConfig {
        AuthConfig {
            secret: "integration-test-secret".to_string(),
            required: true,
            skip_paths: vec!["/health".to_string(), "/metrics".to_string()],
        }
    }

    fn make_token(claims: &AuthClaims, secret: &str) -> String {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn authed_execute_succeeds() {
        let server = SmqlServer::new().with_auth(test_auth_config());
        let app = server.router();

        let claims = AuthClaims {
            sub: "user-1".to_string(),
            role: Some("admin".to_string()),
            exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
        };
        let token = make_token(&claims, "integration-test-secret");

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/execute")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {}", token))
                    .body(Body::from(r#"{"smql":"DEFINE MACHINE Test ( STATES { open, closed } INITIAL STATE open TERMINAL STATES { closed } TRANSITIONS { open -> closed {} } )"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), http::StatusCode::CREATED);
    }

    #[tokio::test]
    async fn unauthed_execute_rejected() {
        let server = SmqlServer::new().with_auth(test_auth_config());
        let app = server.router();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/execute")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"smql":"DEFINE MACHINE Test ( STATES { open } INITIAL STATE open )"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn health_endpoint_skips_auth() {
        let server = SmqlServer::new().with_auth(test_auth_config());
        let app = server.router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), http::StatusCode::OK);
    }
}
