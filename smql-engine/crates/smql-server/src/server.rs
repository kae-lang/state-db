use axum::extract::DefaultBodyLimit;
use axum::middleware::Next;
use axum::response::Response;
use axum::Router;
use smql_catalog::MachineCatalog;
use smql_engine_core::Engine;
use smql_hooks::{EventBus, WebhookClient, WebhookConfig};
use smql_storage::{MemoryStorage, Storage};
use std::sync::Arc;

use crate::handlers;
use crate::metrics::SmqlMetrics;

/// Maximum request body size (1 MB). Protects against oversized payloads.
const MAX_BODY_SIZE: usize = 1024 * 1024;

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
            .layer(DefaultBodyLimit::max(MAX_BODY_SIZE))
            .layer(axum::middleware::from_fn(security_headers))
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

        // Start timer loop to fire expired timeout transitions
        self.state
            .engine
            .start_timer_loop(std::time::Duration::from_secs(1));

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

/// Middleware that adds security headers to all responses.
async fn security_headers(request: axum::extract::Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("x-frame-options", "DENY".parse().unwrap());
    headers.insert(
        "cache-control",
        "no-store, no-cache, must-revalidate".parse().unwrap(),
    );
    response
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
            trust_client_actor: false,
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
            permissions: Vec::new(),
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

    // --- E4: Auth-to-Actor Binding tests ---

    /// Helper: send an authed SMQL request and return the response body as JSON.
    async fn execute_authed(
        app: Router,
        smql: &str,
        claims: &AuthClaims,
        secret: &str,
    ) -> (http::StatusCode, serde_json::Value) {
        let token = make_token(claims, secret);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/execute")
                    .header("content-type", "application/json")
                    .header("authorization", format!("Bearer {}", token))
                    .body(Body::from(
                        serde_json::json!({ "smql": smql }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 100_000)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (status, json)
    }

    fn agent_claims(sub: &str, role: &str, permissions: Vec<String>) -> AuthClaims {
        AuthClaims {
            sub: sub.to_string(),
            role: Some(role.to_string()),
            exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
            permissions,
        }
    }

    const MACHINE_DEF: &str = r#"DEFINE MACHINE AuthFlow (
        STATES { pending, approved, rejected }
        INITIAL STATE pending
        TERMINAL STATES { rejected }
        TRANSITIONS {
            pending -> approved {}
            pending -> rejected {}
        }
    )"#;

    #[tokio::test]
    async fn jwt_sub_records_identity_on_spawn() {
        let server = SmqlServer::new().with_auth(test_auth_config());

        // Define machine
        let app = server.router();
        let claims = agent_claims("agent-jwt-1", "admin", Vec::new());
        let (status, _) = execute_authed(app, MACHINE_DEF, &claims, "integration-test-secret").await;
        assert_eq!(status, http::StatusCode::CREATED);

        // Spawn — JWT sub should be recorded as the actor in the trail
        let app = server.router();
        let (status, json) = execute_authed(
            app,
            r#"SPAWN AuthFlow {}"#,
            &claims,
            "integration-test-secret",
        )
        .await;
        assert_eq!(status, http::StatusCode::CREATED);
        let instance_id = json["result"]["id"].as_str().unwrap().to_string();

        // Verify trail records JWT identity
        let app = server.router();
        let (status, json) = execute_authed(
            app,
            &format!(r#"TRAIL OF "{}""#, instance_id),
            &claims,
            "integration-test-secret",
        )
        .await;
        assert_eq!(status, http::StatusCode::OK);
        let entries = json["result"]["entries"].as_array().unwrap();
        // The spawn trail entry should record "agent-jwt-1" from JWT
        assert_eq!(entries[0]["actor"], "agent-jwt-1");
    }

    #[tokio::test]
    async fn jwt_sub_overrides_as_clause_on_transition() {
        let server = SmqlServer::new().with_auth(test_auth_config());
        let claims = agent_claims("agent-jwt-2", "admin", Vec::new());

        // Define machine
        let app = server.router();
        let (status, _) = execute_authed(app, MACHINE_DEF, &claims, "integration-test-secret").await;
        assert_eq!(status, http::StatusCode::CREATED);

        // Spawn
        let app = server.router();
        let (status, json) = execute_authed(
            app,
            r#"SPAWN AuthFlow {}"#,
            &claims,
            "integration-test-secret",
        )
        .await;
        assert_eq!(status, http::StatusCode::CREATED);
        let instance_id = json["result"]["id"].as_str().unwrap().to_string();

        // Transition with AS "impersonator" — JWT sub should override
        let app = server.router();
        let (status, _) = execute_authed(
            app,
            &format!(r#"TRANSITION AuthFlow "{}" TO approved AS "impersonator""#, instance_id),
            &claims,
            "integration-test-secret",
        )
        .await;
        assert_eq!(status, http::StatusCode::OK);

        // Verify trail records JWT identity
        let app = server.router();
        let (status, json) = execute_authed(
            app,
            &format!(r#"TRAIL OF "{}""#, instance_id),
            &claims,
            "integration-test-secret",
        )
        .await;
        assert_eq!(status, http::StatusCode::OK);
        let entries = json["result"]["entries"].as_array().unwrap();
        // Transition trail entry (index 1) should have "agent-jwt-2" not "impersonator"
        assert_eq!(entries[1]["actor"], "agent-jwt-2");
    }

    #[tokio::test]
    async fn trust_client_actor_allows_as_clause() {
        let config = AuthConfig {
            trust_client_actor: true,
            ..test_auth_config()
        };
        let server = SmqlServer::new().with_auth(config);
        let claims = agent_claims("jwt-user", "admin", Vec::new());

        // Define machine
        let app = server.router();
        let (status, _) = execute_authed(app, MACHINE_DEF, &claims, "integration-test-secret").await;
        assert_eq!(status, http::StatusCode::CREATED);

        // Spawn instance
        let app = server.router();
        let (status, json) = execute_authed(
            app,
            r#"SPAWN AuthFlow {}"#,
            &claims,
            "integration-test-secret",
        )
        .await;
        assert_eq!(status, http::StatusCode::CREATED);
        let instance_id = json["result"]["id"].as_str().unwrap().to_string();

        // Transition with AS "custom-actor" — should be trusted when trust_client_actor=true
        let app = server.router();
        let (status, _) = execute_authed(
            app,
            &format!(r#"TRANSITION AuthFlow "{}" TO approved AS "custom-actor""#, instance_id),
            &claims,
            "integration-test-secret",
        )
        .await;
        assert_eq!(status, http::StatusCode::OK);

        // Verify trail records "custom-actor" (the AS clause), not JWT sub
        let app = server.router();
        let (status, json) = execute_authed(
            app,
            &format!(r#"TRAIL OF "{}""#, instance_id),
            &claims,
            "integration-test-secret",
        )
        .await;
        assert_eq!(status, http::StatusCode::OK);
        let entries = json["result"]["entries"].as_array().unwrap();
        // Transition trail entry (index 1) should record "custom-actor" from AS clause
        assert_eq!(entries[1]["actor"], "custom-actor");
    }

    #[tokio::test]
    async fn actor_capabilities_from_jwt_permissions() {
        // Machine with a guard that checks ACTOR.capabilities
        let machine_def = r#"DEFINE MACHINE CapFlow (
            STATES { pending, approved }
            INITIAL STATE pending
            TERMINAL STATES { approved }
            TRANSITIONS {
                pending -> approved {
                    GUARD: "approve" IN ACTOR.capabilities
                }
            }
        )"#;

        let server = SmqlServer::new().with_auth(test_auth_config());

        // Agent with "approve" permission
        let claims_with_perm = agent_claims("approver-agent", "admin", vec!["approve".to_string()]);

        // Define machine
        let app = server.router();
        let (status, _) = execute_authed(app, machine_def, &claims_with_perm, "integration-test-secret").await;
        assert_eq!(status, http::StatusCode::CREATED);

        // Spawn
        let app = server.router();
        let (status, json) = execute_authed(
            app,
            r#"SPAWN CapFlow {}"#,
            &claims_with_perm,
            "integration-test-secret",
        )
        .await;
        assert_eq!(status, http::StatusCode::CREATED);
        let instance_id = json["result"]["id"].as_str().unwrap().to_string();

        // Transition with "approve" capability — should succeed
        let app = server.router();
        let (status, _) = execute_authed(
            app,
            &format!(r#"TRANSITION CapFlow "{}" TO approved"#, instance_id),
            &claims_with_perm,
            "integration-test-secret",
        )
        .await;
        assert_eq!(status, http::StatusCode::OK);

        // Now test that agent WITHOUT "approve" permission gets denied
        let claims_no_perm = agent_claims("other-agent", "admin", Vec::new());

        // Spawn another instance
        let app = server.router();
        let (status, json) = execute_authed(
            app,
            r#"SPAWN CapFlow {}"#,
            &claims_no_perm,
            "integration-test-secret",
        )
        .await;
        assert_eq!(status, http::StatusCode::CREATED);
        let instance_id2 = json["result"]["id"].as_str().unwrap().to_string();

        // Transition without capability — should fail (409 Conflict)
        let app = server.router();
        let (status, _) = execute_authed(
            app,
            &format!(r#"TRANSITION CapFlow "{}" TO approved"#, instance_id2),
            &claims_no_perm,
            "integration-test-secret",
        )
        .await;
        assert_eq!(status, http::StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn non_auth_mode_uses_as_clause() {
        // Server without auth — regression test
        // TRANSITION AS clause should work when no auth middleware is present
        let server = SmqlServer::new();

        // Define machine (no auth)
        let app = server.router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/execute")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "smql": MACHINE_DEF }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::CREATED);

        // Spawn instance
        let app = server.router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/execute")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "smql": r#"SPAWN AuthFlow {}"# }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::CREATED);
        let body = axum::body::to_bytes(response.into_body(), 100_000)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let instance_id = json["result"]["id"].as_str().unwrap().to_string();

        // Transition with AS clause — should be recorded in trail
        let app = server.router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/execute")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "smql": format!(r#"TRANSITION AuthFlow "{}" TO approved AS "my-agent""#, instance_id) }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);

        // Verify trail records "my-agent" from AS clause
        let app = server.router();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/execute")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "smql": format!(r#"TRAIL OF "{}""#, instance_id) }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 100_000)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let entries = json["result"]["entries"].as_array().unwrap();
        // Transition trail entry (index 1) should have "my-agent" from AS clause
        assert_eq!(entries[1]["actor"], "my-agent");
    }
}
