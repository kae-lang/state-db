use crate::error::{SdkError, SdkResult};
use crate::find::{AggregateBuilder, FindBuilder};
use crate::smql_fmt;
use crate::subscription::Subscription;
use crate::types::*;
use std::time::Duration;

/// Builder for configuring an `SmqlClient`.
pub struct SmqlClientBuilder {
    base_url: String,
    timeout: Option<Duration>,
}

impl SmqlClientBuilder {
    /// Set the connection/request timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Build the client.
    pub fn build(self) -> SdkResult<SmqlClient> {
        let mut builder = reqwest::Client::builder();
        if let Some(t) = self.timeout {
            builder = builder.timeout(t);
        }
        let http = builder.build().map_err(SdkError::Http)?;
        // Normalize: strip trailing slash
        let base_url = self.base_url.trim_end_matches('/').to_string();
        Ok(SmqlClient { http, base_url })
    }
}

/// HTTP client for the SMQL server.
pub struct SmqlClient {
    pub(crate) http: reqwest::Client,
    pub(crate) base_url: String,
}

impl SmqlClient {
    /// Create a new client connected to the given base URL.
    pub fn new(base_url: &str) -> SdkResult<Self> {
        Self::builder(base_url).build()
    }

    /// Create a builder for more configuration options.
    pub fn builder(base_url: &str) -> SmqlClientBuilder {
        SmqlClientBuilder {
            base_url: base_url.to_string(),
            timeout: None,
        }
    }

    /// Execute raw SMQL and return the response.
    pub async fn execute(&self, smql: &str) -> SdkResult<ExecuteResponse> {
        let resp = self
            .http
            .post(format!("{}/execute", self.base_url))
            .json(&serde_json::json!({ "smql": smql }))
            .send()
            .await?;

        let status = resp.status();
        let body: ExecuteResponse = resp.json().await.map_err(|e| {
            SdkError::Deserialize(format!("Failed to parse response: {}", e))
        })?;

        if !body.success {
            let msg = body.error.unwrap_or_default();
            if status == reqwest::StatusCode::NOT_FOUND {
                return Err(SdkError::NotFound(msg));
            }
            if status == reqwest::StatusCode::CONFLICT {
                return Err(SdkError::TransitionDenied(msg));
            }
            return Err(SdkError::Server(msg));
        }

        Ok(body)
    }

    /// Define a machine from SMQL source.
    pub async fn define_machine(&self, smql: &str) -> SdkResult<DefineResult> {
        let resp = self.execute(smql).await?;
        let result = resp.result.unwrap_or_default();
        let action = result["action"].as_str().unwrap_or("").to_string();
        Ok(DefineResult {
            action,
            warnings: resp.warnings,
        })
    }

    /// Spawn a new instance.
    pub async fn spawn(
        &self,
        machine: &str,
        data: serde_json::Value,
    ) -> SdkResult<InstanceResponse> {
        let smql = smql_fmt::format_spawn(machine, &data);
        let resp = self.execute(&smql).await?;
        let result = resp.result.ok_or_else(|| {
            SdkError::Deserialize("Missing result in spawn response".to_string())
        })?;
        serde_json::from_value(result).map_err(|e| SdkError::Deserialize(e.to_string()))
    }

    /// Transition an instance to a new state.
    pub async fn transition(
        &self,
        instance_id: &str,
        to_state: &str,
        opts: TransitionOptions,
    ) -> SdkResult<TransitionResponse> {
        let smql = smql_fmt::format_transition(instance_id, to_state, &opts);
        let resp = self.execute(&smql).await?;
        let result = resp.result.ok_or_else(|| {
            SdkError::Deserialize("Missing result in transition response".to_string())
        })?;
        serde_json::from_value(result).map_err(|e| SdkError::Deserialize(e.to_string()))
    }

    /// Try to transition; returns `None` if guard fails instead of an error.
    pub async fn try_transition(
        &self,
        instance_id: &str,
        to_state: &str,
        opts: TransitionOptions,
    ) -> SdkResult<Option<TransitionResponse>> {
        let smql = smql_fmt::format_try_transition(instance_id, to_state, &opts);
        let resp = self.execute(&smql).await?;
        let result = resp.result.ok_or_else(|| {
            SdkError::Deserialize("Missing result in try_transition response".to_string())
        })?;
        let transitioned = result["transitioned"].as_bool().unwrap_or(false);
        if !transitioned {
            return Ok(None);
        }
        let tr: TransitionResponse =
            serde_json::from_value(result).map_err(|e| SdkError::Deserialize(e.to_string()))?;
        Ok(Some(tr))
    }

    /// Get an instance by ID (via REST endpoint).
    pub async fn get_instance(&self, id: &str) -> SdkResult<InstanceResponse> {
        let url = format!("{}/instances/{}", self.base_url, id);
        let resp = self.http.get(&url).send().await?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            let body: serde_json::Value =
                serde_json::from_str(&body_text).unwrap_or_default();
            let msg = body["error"]
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| format!("HTTP {}: {}", status, body_text));
            if status == reqwest::StatusCode::NOT_FOUND {
                return Err(SdkError::NotFound(msg));
            }
            return Err(SdkError::Server(msg));
        }

        let inst: InstanceResponse = resp
            .json()
            .await
            .map_err(|e| SdkError::Deserialize(e.to_string()))?;
        Ok(inst)
    }

    /// List all registered machine names.
    pub async fn list_machines(&self) -> SdkResult<Vec<String>> {
        let resp = self
            .http
            .get(format!("{}/machines", self.base_url))
            .send()
            .await?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SdkError::Deserialize(e.to_string()))?;

        let names = body["machines"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Ok(names)
    }

    /// Get machine definition info.
    pub async fn get_machine(&self, name: &str) -> SdkResult<MachineInfo> {
        let resp = self
            .http
            .get(format!("{}/machines/{}", self.base_url, name))
            .send()
            .await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(SdkError::NotFound(format!("Machine '{}' not found", name)));
        }

        let info: MachineInfo = resp
            .json()
            .await
            .map_err(|e| SdkError::Deserialize(e.to_string()))?;
        Ok(info)
    }

    /// Get trail entries for an instance.
    pub async fn trail(&self, instance_id: &str) -> SdkResult<Vec<TrailEntryResponse>> {
        let smql = smql_fmt::format_trail(instance_id);
        let resp = self.execute(&smql).await?;
        let result = resp.result.unwrap_or_default();
        let entries = result["entries"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        Ok(entries)
    }

    /// Check server health.
    pub async fn health(&self) -> SdkResult<bool> {
        let resp = self
            .http
            .get(format!("{}/health", self.base_url))
            .send()
            .await?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SdkError::Deserialize(e.to_string()))?;

        Ok(body["status"] == "ok")
    }

    /// Start building a FIND query.
    pub fn find(&self, machine: &str) -> FindBuilder<'_> {
        FindBuilder::new(self, machine)
    }

    /// Start building an AGGREGATE query.
    pub fn aggregate(&self, machine: &str) -> AggregateBuilder<'_> {
        AggregateBuilder::new(self, machine)
    }

    /// Subscribe to server events via WebSocket.
    pub async fn subscribe(&self, machine: Option<&str>) -> SdkResult<Subscription> {
        let ws_url = self
            .base_url
            .replacen("http://", "ws://", 1)
            .replacen("https://", "wss://", 1);
        let mut url = format!("{}/subscribe", ws_url);
        if let Some(m) = machine {
            url.push_str(&format!("?machine={}", m));
        }
        Subscription::connect(&url).await
    }
}
