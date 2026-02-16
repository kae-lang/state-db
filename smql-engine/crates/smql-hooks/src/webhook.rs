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
pub fn build_webhook_body(
    ctx: &HookContext,
    payload: Option<&Value>,
) -> serde_json::Value {
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
        Value::List(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
        Value::Map(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::Set(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
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
        let payload = Value::Map(BTreeMap::from([
            ("key".to_string(), Value::Text("val".to_string())),
        ]));
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
        assert_eq!(value_to_json(&Value::Float(3.14)), serde_json::json!(3.14));
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
}
