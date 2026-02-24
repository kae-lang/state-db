// SMQL Server — JWT Authentication middleware

use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};

/// JWT claims extracted from the Authorization header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthClaims {
    /// Subject — typically the user ID.
    pub sub: String,
    /// Role — optional role for RBAC.
    #[serde(default)]
    pub role: Option<String>,
    /// Expiration time (Unix timestamp).
    pub exp: usize,
    /// Capabilities/permissions — derived from JWT `permissions` or `scope` claim.
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Configuration for JWT authentication.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// The secret key used to verify JWT signatures (HS256).
    pub secret: String,
    /// Whether authentication is required. If false, unauthenticated requests pass through.
    pub required: bool,
    /// Paths that skip authentication entirely.
    pub skip_paths: Vec<String>,
    /// When true, the SMQL `AS` clause is trusted even when JWT auth is present.
    /// When false (default), the JWT identity overrides any client-provided AS clause.
    pub trust_client_actor: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            secret: String::new(),
            required: true,
            skip_paths: vec!["/health".to_string(), "/metrics".to_string()],
            trust_client_actor: false,
        }
    }
}

/// Axum middleware for JWT authentication.
///
/// Flow:
/// 1. Check skip_paths → pass through
/// 2. Extract `Authorization: Bearer <token>`
/// 3. Decode JWT with jsonwebtoken
/// 4. On success → insert AuthClaims into request extensions
/// 5. On failure + required=true → 401. On failure + required=false → pass through
pub async fn auth_middleware(request: Request, next: Next) -> Response {
    // Get auth config from extensions
    let config = match request.extensions().get::<AuthConfig>() {
        Some(config) => config.clone(),
        None => {
            // No config means auth is not configured — pass through
            return next.run(request).await;
        }
    };

    let path = request.uri().path().to_string();

    // Check skip paths — exact match to prevent bypass via prefix collision
    // (e.g., "/healthz_leak" must NOT match skip_path "/health")
    if config.skip_paths.iter().any(|p| path == *p) {
        return next.run(request).await;
    }

    // Extract Bearer token
    let token = request
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|t| t.trim().to_string());

    match token {
        Some(token) if !token.is_empty() => {
            // Reject empty secrets — misconfiguration guard
            if config.secret.is_empty() {
                tracing::error!("JWT auth configured with empty secret — rejecting all tokens");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    axum::Json(serde_json::json!({
                        "error": "Authentication misconfigured",
                    })),
                )
                    .into_response();
            }

            // Decode JWT — explicitly require HS256 to prevent algorithm confusion attacks
            let key = DecodingKey::from_secret(config.secret.as_bytes());
            let mut validation = Validation::new(Algorithm::HS256);
            validation.validate_exp = true;

            match decode::<AuthClaims>(&token, &key, &validation) {
                Ok(token_data) => {
                    let mut request = request;
                    request.extensions_mut().insert(token_data.claims);
                    next.run(request).await
                }
                Err(_e) => {
                    if config.required {
                        tracing::debug!("JWT validation failed: {}", _e);
                        (
                            StatusCode::UNAUTHORIZED,
                            axum::Json(serde_json::json!({
                                "error": "Invalid or expired token",
                            })),
                        )
                            .into_response()
                    } else {
                        // Optional auth: pass through without claims
                        next.run(request).await
                    }
                }
            }
        }
        _ => {
            // No token
            if config.required {
                (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({
                        "error": "Missing Authorization header",
                    })),
                )
                    .into_response()
            } else {
                // Optional auth: pass through without claims
                next.run(request).await
            }
        }
    }
}

/// Extension layer that injects AuthConfig into all requests.
#[derive(Clone)]
pub struct AuthConfigLayer {
    pub config: AuthConfig,
}

impl<S> tower::Layer<S> for AuthConfigLayer {
    type Service = AuthConfigService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthConfigService {
            inner,
            config: self.config.clone(),
        }
    }
}

#[derive(Clone)]
pub struct AuthConfigService<S> {
    inner: S,
    config: AuthConfig,
}

impl<S> tower::Service<Request<Body>> for AuthConfigService<S>
where
    S: tower::Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        request.extensions_mut().insert(self.config.clone());
        let future = self.inner.call(request);
        Box::pin(future)
    }
}

/// Extract optional auth claims from request extensions.
/// Use this in handlers to get the authenticated user's claims.
pub fn extract_claims(extensions: &axum::http::Extensions) -> Option<&AuthClaims> {
    extensions.get::<AuthClaims>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use http::header::AUTHORIZATION;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use tower::ServiceExt;

    fn test_config() -> AuthConfig {
        AuthConfig {
            secret: "test-secret-key".to_string(),
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

    fn valid_claims() -> AuthClaims {
        AuthClaims {
            sub: "user-123".to_string(),
            role: Some("admin".to_string()),
            exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
            permissions: Vec::new(),
        }
    }

    fn expired_claims() -> AuthClaims {
        AuthClaims {
            sub: "user-123".to_string(),
            role: None,
            exp: (chrono::Utc::now() - chrono::Duration::hours(1)).timestamp() as usize,
            permissions: Vec::new(),
        }
    }

    async fn handler_with_claims(request: Request<Body>) -> impl IntoResponse {
        let claims = request.extensions().get::<AuthClaims>();
        match claims {
            Some(c) => axum::Json(serde_json::json!({
                "sub": c.sub,
                "role": c.role,
            }))
            .into_response(),
            None => axum::Json(serde_json::json!({
                "sub": null,
            }))
            .into_response(),
        }
    }

    fn build_test_router(config: AuthConfig) -> Router {
        Router::new()
            .route("/test", get(handler_with_claims))
            .route("/health", get(|| async { "ok" }))
            .layer(middleware::from_fn(auth_middleware))
            .layer(AuthConfigLayer { config })
    }

    #[tokio::test]
    async fn valid_jwt_extracts_claims() {
        let config = test_config();
        let app = build_test_router(config.clone());

        let token = make_token(&valid_claims(), &config.secret);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 10000)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["sub"], "user-123");
        assert_eq!(json["role"], "admin");
    }

    #[tokio::test]
    async fn expired_jwt_returns_401() {
        let config = test_config();
        let app = build_test_router(config.clone());

        let token = make_token(&expired_claims(), &config.secret);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn missing_header_required_returns_401() {
        let config = test_config();
        let app = build_test_router(config);

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn missing_header_optional_passes_through() {
        let config = AuthConfig {
            required: false,
            ..test_config()
        };
        let app = build_test_router(config);

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 10000)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["sub"].is_null());
    }

    #[tokio::test]
    async fn skip_paths_bypass_auth() {
        let config = test_config();
        let app = build_test_router(config);

        // /health should bypass auth
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn malformed_token_returns_401() {
        let config = test_config();
        let app = build_test_router(config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(AUTHORIZATION, "Bearer not-a-real-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn empty_bearer_returns_401() {
        let config = test_config();
        let app = build_test_router(config);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(AUTHORIZATION, "Bearer ")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn wrong_secret_returns_401() {
        let config = test_config();
        let app = build_test_router(config);

        let token = make_token(&valid_claims(), "wrong-secret");
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn skip_path_prefix_does_not_bypass_auth() {
        let config = test_config();
        let app = Router::new()
            .route("/health_secret", get(handler_with_claims))
            .route("/health", get(|| async { "ok" }))
            .layer(middleware::from_fn(auth_middleware))
            .layer(AuthConfigLayer { config });

        // /health_secret should NOT bypass auth even though it starts with "/health"
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health_secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
