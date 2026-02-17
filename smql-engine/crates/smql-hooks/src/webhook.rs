// SMQL Hooks — Webhook HTTP client with retry logic

use crate::HookContext;
use smql_ast::value::Value;
use std::time::Duration;

/// Configuration for the webhook HTTP client.
#[derive(Debug, Clone)]
pub struct WebhookConfig {
    /// HTTP request timeout.
    pub timeout: Duration,
    /// Maximum number of retry attempts (0 = no retries).
    pub max_retries: u32,
    /// Delay between retries.
    pub retry_delay: Duration,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            max_retries: 2,
            retry_delay: Duration::from_secs(1),
        }
    }
}

/// HTTP client for executing webhook actions.
pub struct WebhookClient {
    client: reqwest::Client,
    config: WebhookConfig,
}

impl WebhookClient {
    pub fn new(config: WebhookConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .build()
            .unwrap_or_default();
        Self { client, config }
    }

    /// Execute a webhook POST request with retry logic.
    ///
    /// Retries on 5xx responses and network errors.
    /// Does NOT retry on 4xx (client errors).
    pub async fn execute(
        &self,
        url: &str,
        ctx: &HookContext,
        payload: Option<&Value>,
    ) -> Result<(), WebhookError> {
        let body = build_webhook_body(ctx, payload);
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                tokio::time::sleep(self.config.retry_delay).await;
                tracing::debug!(attempt, url, "retrying webhook");
            }

            match self.client.post(url).json(&body).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return Ok(());
                    } else if status.is_client_error() {
                        // 4xx: don't retry
                        let body_text = response.text().await.unwrap_or_default();
                        return Err(WebhookError::ClientError {
                            status: status.as_u16(),
                            body: body_text,
                        });
                    } else {
                        // 5xx: retry
                        let body_text = response.text().await.unwrap_or_default();
                        last_error = Some(WebhookError::ServerError {
                            status: status.as_u16(),
                            body: body_text,
                        });
                    }
                }
                Err(e) => {
                    if e.is_timeout() {
                        last_error = Some(WebhookError::Timeout);
                    } else {
                        last_error = Some(WebhookError::Network {
                            message: e.to_string(),
                        });
                    }
                }
            }
        }

        Err(last_error.unwrap_or(WebhookError::Network {
            message: "Unknown error".to_string(),
        }))
    }
}

/// Error types for webhook execution.
#[derive(Debug, Clone, thiserror::Error)]
pub enum WebhookError {
    #[error("Webhook client error (HTTP {status}): {body}")]
    ClientError { status: u16, body: String },

    #[error("Webhook server error (HTTP {status}): {body}")]
    ServerError { status: u16, body: String },

    #[error("Webhook request timed out")]
    Timeout,

    #[error("Webhook network error: {message}")]
    Network { message: String },
}

/// Build the JSON body for a webhook POST request.
pub fn build_webhook_body(ctx: &HookContext, payload: Option<&Value>) -> serde_json::Value {
    let mut body = serde_json::json!({
        "event_type": "transition",
        "instance_id": ctx.instance_id,
        "machine": ctx.machine,
        "from_state": ctx.from_state,
        "to_state": ctx.to_state,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    if let Some(payload) = payload {
        body["payload"] = value_to_json(payload);
    }

    if let Some(actor) = &ctx.actor {
        body["actor"] = serde_json::Value::String(actor.clone());
    }

    body
}

/// Convert an SMQL Value to a serde_json::Value.
pub fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Int(i) => serde_json::json!(i),
        Value::Float(f) => serde_json::json!(f),
        Value::Text(s) => serde_json::Value::String(s.clone()),
        Value::List(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Map(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::Set(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Date(d) => serde_json::Value::String(d.to_string()),
        Value::DateTime(dt) => serde_json::Value::String(dt.to_rfc3339()),
        Value::Duration(d) => serde_json::Value::String(d.to_string()),
        Value::Uuid(u) => serde_json::Value::String(u.to_string()),
        Value::Money(amount, currency) => {
            serde_json::json!({"amount": amount, "currency": currency})
        }
        Value::Ref(machine, id) => {
            serde_json::json!({"$ref": machine, "$id": id})
        }
        Value::Blob(bytes) => {
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            serde_json::Value::String(encoded)
        }
        Value::Json(raw) => raw.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, HashMap};

    fn test_ctx() -> HookContext {
        let mut data = HashMap::new();
        data.insert("title".to_string(), Value::Text("Bug fix".to_string()));
        HookContext {
            instance_id: "INST001".to_string(),
            machine: "Ticket".to_string(),
            from_state: "open".to_string(),
            to_state: "in_progress".to_string(),
            data,
            actor: Some("alice".to_string()),
            memo: None,
        }
    }

    #[test]
    fn webhook_body_structure() {
        let ctx = test_ctx();
        let payload = Value::Map(BTreeMap::from([(
            "key".to_string(),
            Value::Text("val".to_string()),
        )]));
        let body = build_webhook_body(&ctx, Some(&payload));

        assert_eq!(body["event_type"], "transition");
        assert_eq!(body["instance_id"], "INST001");
        assert_eq!(body["machine"], "Ticket");
        assert_eq!(body["from_state"], "open");
        assert_eq!(body["to_state"], "in_progress");
        assert_eq!(body["actor"], "alice");
        assert!(body["timestamp"].is_string());
        assert_eq!(body["payload"]["key"], "val");
    }

    #[test]
    fn webhook_body_without_payload_or_actor() {
        let ctx = HookContext {
            instance_id: "I2".to_string(),
            machine: "Order".to_string(),
            from_state: "new".to_string(),
            to_state: "paid".to_string(),
            data: HashMap::new(),
            actor: None,
            memo: None,
        };
        let body = build_webhook_body(&ctx, None);

        assert_eq!(body["event_type"], "transition");
        assert!(body.get("payload").is_none());
        assert!(body.get("actor").is_none());
    }

    #[test]
    fn value_to_json_covers_all_types() {
        assert_eq!(value_to_json(&Value::Null), serde_json::Value::Null);
        assert_eq!(value_to_json(&Value::Bool(true)), serde_json::json!(true));
        assert_eq!(value_to_json(&Value::Int(42)), serde_json::json!(42));
        assert_eq!(
            value_to_json(&Value::Float(3.125)),
            serde_json::json!(3.125)
        );
        assert_eq!(
            value_to_json(&Value::Text("hi".into())),
            serde_json::json!("hi")
        );
        assert_eq!(
            value_to_json(&Value::List(vec![Value::Int(1), Value::Int(2)])),
            serde_json::json!([1, 2])
        );
        assert_eq!(
            value_to_json(&Value::Set(vec![Value::Text("a".into())])),
            serde_json::json!(["a"])
        );

        let map = BTreeMap::from([("k".to_string(), Value::Int(1))]);
        assert_eq!(value_to_json(&Value::Map(map)), serde_json::json!({"k": 1}));

        let money = Value::Money(999, "USD".into());
        let j = value_to_json(&money);
        assert_eq!(j["amount"], 999);
        assert_eq!(j["currency"], "USD");

        let ref_val = Value::Ref("Order".into(), "id123".into());
        let j = value_to_json(&ref_val);
        assert_eq!(j["$ref"], "Order");
        assert_eq!(j["$id"], "id123");
    }

    #[test]
    fn value_to_json_date_datetime_duration() {
        use chrono::{NaiveDate, Utc};
        use smql_ast::value::SmqlDuration;

        let d = Value::Date(NaiveDate::from_ymd_opt(2025, 6, 15).unwrap());
        let j = value_to_json(&d);
        assert_eq!(j.as_str().unwrap(), "2025-06-15");

        let dt = Value::DateTime(Utc::now());
        let j = value_to_json(&dt);
        assert!(j.as_str().unwrap().contains("T"));

        let dur = Value::Duration(SmqlDuration::from_hours(2));
        let j = value_to_json(&dur);
        assert_eq!(j.as_str().unwrap(), "2h");
    }

    #[test]
    fn value_to_json_uuid() {
        // Construct UUID from bytes to avoid needing uuid crate directly
        let uuid_str = "00000000-0000-0000-0000-000000000000";
        let j = value_to_json(&Value::Text(uuid_str.into()));
        assert_eq!(j.as_str().unwrap(), uuid_str);
    }

    #[test]
    fn value_to_json_blob_base64() {
        let blob = Value::Blob(vec![72, 101, 108, 108, 111]); // "Hello"
        let j = value_to_json(&blob);
        let encoded = j.as_str().unwrap();
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        assert_eq!(decoded, vec![72, 101, 108, 108, 111]);
    }

    #[test]
    fn value_to_json_raw_json() {
        let raw = serde_json::json!({"nested": [1, 2, 3]});
        let j = value_to_json(&Value::Json(raw.clone()));
        assert_eq!(j, raw);
    }

    #[test]
    fn webhook_config_default() {
        let config = WebhookConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(10));
        assert_eq!(config.max_retries, 2);
        assert_eq!(config.retry_delay, Duration::from_secs(1));
    }

    #[test]
    fn webhook_client_new() {
        let config = WebhookConfig {
            timeout: Duration::from_secs(5),
            max_retries: 0,
            retry_delay: Duration::from_millis(100),
        };
        let _client = WebhookClient::new(config);
    }

    #[test]
    fn webhook_error_display() {
        let e = WebhookError::ClientError {
            status: 400,
            body: "bad request".into(),
        };
        assert!(e.to_string().contains("400"));

        let e = WebhookError::ServerError {
            status: 500,
            body: "internal error".into(),
        };
        assert!(e.to_string().contains("500"));

        let e = WebhookError::Timeout;
        assert!(e.to_string().contains("timed out"));

        let e = WebhookError::Network {
            message: "connection refused".into(),
        };
        assert!(e.to_string().contains("connection refused"));
    }

    #[test]
    fn value_to_json_empty_collections() {
        assert_eq!(value_to_json(&Value::List(vec![])), serde_json::json!([]));
        assert_eq!(value_to_json(&Value::Set(vec![])), serde_json::json!([]));
        assert_eq!(
            value_to_json(&Value::Map(BTreeMap::new())),
            serde_json::json!({})
        );
    }

    #[test]
    fn value_to_json_nested_map() {
        let mut inner = BTreeMap::new();
        inner.insert("x".to_string(), Value::Int(1));
        let mut outer = BTreeMap::new();
        outer.insert("inner".to_string(), Value::Map(inner));
        let j = value_to_json(&Value::Map(outer));
        assert_eq!(j["inner"]["x"], 1);
    }

    // ---------------------------------------------------------------
    // Additional coverage tests — webhook execution paths
    // ---------------------------------------------------------------

    /// Webhook execute against an unreachable server (port 1) — should return Network error.
    #[tokio::test]
    async fn webhook_execute_unreachable_server() {
        let config = WebhookConfig {
            timeout: Duration::from_secs(2),
            max_retries: 0,
            retry_delay: Duration::from_millis(10),
        };
        let client = WebhookClient::new(config);
        let ctx = test_ctx();

        let result = client
            .execute("http://127.0.0.1:1/webhook", &ctx, None)
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            WebhookError::Network { message } => {
                assert!(
                    !message.is_empty(),
                    "Network error message should not be empty"
                );
            }
            WebhookError::Timeout => {
                // Also acceptable — some systems timeout instead of refusing
            }
            other => panic!(
                "Expected Network or Timeout error, got: {:?}",
                other
            ),
        }
    }

    /// Webhook execute with retries against an unreachable server — should retry and still fail.
    #[tokio::test]
    async fn webhook_execute_with_retries_unreachable() {
        let config = WebhookConfig {
            timeout: Duration::from_secs(1),
            max_retries: 2,
            retry_delay: Duration::from_millis(50),
        };
        let client = WebhookClient::new(config);
        let ctx = test_ctx();

        let start = std::time::Instant::now();
        let result = client
            .execute("http://127.0.0.1:1/webhook", &ctx, None)
            .await;
        let elapsed = start.elapsed();

        assert!(result.is_err());
        // With 2 retries and 50ms delay, should take at least ~100ms (2 retry delays)
        assert!(
            elapsed >= Duration::from_millis(80),
            "Expected retries to add delay, elapsed: {:?}",
            elapsed
        );
    }

    /// Webhook execute with payload against unreachable server.
    #[tokio::test]
    async fn webhook_execute_with_payload_unreachable() {
        let config = WebhookConfig {
            timeout: Duration::from_secs(1),
            max_retries: 0,
            retry_delay: Duration::from_millis(10),
        };
        let client = WebhookClient::new(config);
        let ctx = test_ctx();
        let payload = Value::Map(BTreeMap::from([
            ("key".to_string(), Value::Text("value".to_string())),
            ("count".to_string(), Value::Int(42)),
        ]));

        let result = client
            .execute("http://127.0.0.1:1/webhook", &ctx, Some(&payload))
            .await;
        assert!(result.is_err());
    }

    /// Webhook with very short timeout to exercise the timeout error path.
    #[tokio::test]
    async fn webhook_execute_short_timeout() {
        let config = WebhookConfig {
            // 1ms timeout — almost certainly will time out on any real request
            timeout: Duration::from_millis(1),
            max_retries: 0,
            retry_delay: Duration::from_millis(10),
        };
        let client = WebhookClient::new(config);
        let ctx = test_ctx();

        // Use a non-routable IP to maximize chance of timeout rather than connection refused
        let result = client
            .execute("http://192.0.2.1:9999/webhook", &ctx, None)
            .await;
        assert!(result.is_err());
        // Should be either Timeout or Network error depending on OS behavior
        match result.unwrap_err() {
            WebhookError::Timeout | WebhookError::Network { .. } => {}
            other => panic!("Expected Timeout or Network error, got: {:?}", other),
        }
    }

    /// Webhook against a local TCP listener that returns 4xx — should fail immediately with ClientError.
    #[tokio::test]
    async fn webhook_execute_4xx_no_retry() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        // Bind a local TCP listener that returns HTTP 400
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Spawn a handler that returns 400 Bad Request
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            // Read request (just consume it)
            let mut buf = [0u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
            // Send HTTP 400 response
            let response = "HTTP/1.1 400 Bad Request\r\nContent-Length: 11\r\n\r\nbad request";
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
        });

        let config = WebhookConfig {
            timeout: Duration::from_secs(5),
            max_retries: 2, // Should NOT retry on 4xx
            retry_delay: Duration::from_millis(50),
        };
        let client = WebhookClient::new(config);
        let ctx = test_ctx();

        let url = format!("http://127.0.0.1:{}/webhook", addr.port());
        let start = std::time::Instant::now();
        let result = client.execute(&url, &ctx, None).await;
        let elapsed = start.elapsed();

        assert!(result.is_err());
        match result.unwrap_err() {
            WebhookError::ClientError { status, body } => {
                assert_eq!(status, 400);
                assert_eq!(body, "bad request");
            }
            other => panic!("Expected ClientError, got: {:?}", other),
        }

        // Should NOT have retried (no retry delay elapsed)
        assert!(
            elapsed < Duration::from_millis(100),
            "4xx should not retry, elapsed: {:?}",
            elapsed
        );

        handle.await.unwrap();
    }

    /// Webhook against a local TCP listener that returns 5xx — should retry and return ServerError.
    #[tokio::test]
    async fn webhook_execute_5xx_retries() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        // Bind a local TCP listener that always returns 500
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Spawn a handler that returns 500 for every connection (need max_retries + 1 accepts)
        let handle = tokio::spawn(async move {
            for _ in 0..3 {
                // initial + 2 retries
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 4096];
                let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
                let response =
                    "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 12\r\n\r\nserver error";
                stream.write_all(response.as_bytes()).await.unwrap();
                stream.flush().await.unwrap();
            }
        });

        let config = WebhookConfig {
            timeout: Duration::from_secs(5),
            max_retries: 2,
            retry_delay: Duration::from_millis(50),
        };
        let client = WebhookClient::new(config);
        let ctx = test_ctx();

        let url = format!("http://127.0.0.1:{}/webhook", addr.port());
        let start = std::time::Instant::now();
        let result = client.execute(&url, &ctx, None).await;
        let elapsed = start.elapsed();

        assert!(result.is_err());
        match result.unwrap_err() {
            WebhookError::ServerError { status, body } => {
                assert_eq!(status, 500);
                assert_eq!(body, "server error");
            }
            other => panic!("Expected ServerError, got: {:?}", other),
        }

        // Should have retried twice with 50ms delays
        assert!(
            elapsed >= Duration::from_millis(80),
            "5xx should have retried, elapsed: {:?}",
            elapsed
        );

        handle.await.unwrap();
    }

    /// Webhook against a local TCP listener that returns 200 — should succeed.
    #[tokio::test]
    async fn webhook_execute_success_200() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut stream, &mut buf).await;
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
        });

        let config = WebhookConfig {
            timeout: Duration::from_secs(5),
            max_retries: 0,
            retry_delay: Duration::from_millis(10),
        };
        let client = WebhookClient::new(config);
        let ctx = test_ctx();
        let payload = Value::Map(BTreeMap::from([
            ("event".to_string(), Value::Text("created".to_string())),
        ]));

        let url = format!("http://127.0.0.1:{}/webhook", addr.port());
        let result = client.execute(&url, &ctx, Some(&payload)).await;
        assert!(result.is_ok());

        handle.await.unwrap();
    }

    /// Webhook body includes actor when present and omits when not.
    #[test]
    fn webhook_body_with_and_without_actor() {
        let ctx_with_actor = test_ctx();
        let body1 = build_webhook_body(&ctx_with_actor, None);
        assert_eq!(body1["actor"], "alice");

        let ctx_without_actor = HookContext {
            instance_id: "I3".to_string(),
            machine: "M".to_string(),
            from_state: "a".to_string(),
            to_state: "b".to_string(),
            data: HashMap::new(),
            actor: None,
            memo: None,
        };
        let body2 = build_webhook_body(&ctx_without_actor, None);
        assert!(body2.get("actor").is_none());
    }

    /// value_to_json for Value::Uuid — exercises the Uuid branch (line 159).
    #[test]
    fn value_to_json_uuid_variant() {
        let uuid = uuid::Uuid::nil();
        let j = value_to_json(&Value::Uuid(uuid));
        assert_eq!(j.as_str().unwrap(), "00000000-0000-0000-0000-000000000000");
    }

    /// Webhook body with various payload types.
    #[test]
    fn webhook_body_with_complex_payload() {
        let ctx = test_ctx();
        let payload = Value::List(vec![
            Value::Int(1),
            Value::Text("two".to_string()),
            Value::Bool(false),
        ]);
        let body = build_webhook_body(&ctx, Some(&payload));
        assert!(body["payload"].is_array());
        assert_eq!(body["payload"][0], 1);
        assert_eq!(body["payload"][1], "two");
        assert_eq!(body["payload"][2], false);
    }
}
