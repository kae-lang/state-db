use serde::{Deserialize, Serialize};

/// Raw response from POST /execute.
#[derive(Debug, Deserialize)]
pub struct ExecuteResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub warnings: Option<Vec<String>>,
    /// Error metadata for agents: retryable flag and error category.
    #[serde(default)]
    pub error_meta: Option<ErrorMeta>,
}

/// Metadata about an error to help agents decide how to respond.
#[derive(Debug, Clone, Deserialize)]
pub struct ErrorMeta {
    /// Whether the operation can be retried.
    pub retryable: bool,
    /// Error category: "conflict", "storage", "timeout", "internal", "validation".
    pub category: String,
}

/// Structured detail from a denied transition.
#[derive(Debug, Clone, Deserialize)]
pub struct TransitionDeniedDetail {
    pub instance_id: String,
    pub from_state: String,
    pub to_state: String,
    pub guard_failures: Vec<GuardFailureDetail>,
    #[serde(default)]
    pub recovery_options: Vec<RecoveryOptionDetail>,
    #[serde(default)]
    pub llm_prompt: Option<String>,
    pub hint: Option<String>,
}

/// A single guard failure.
#[derive(Debug, Clone, Deserialize)]
pub struct GuardFailureDetail {
    pub guard_expr: String,
    pub actual_value: Option<String>,
    pub expected: Option<String>,
    pub hint: Option<String>,
}

/// An actionable recovery option for AI agents.
#[derive(Debug, Clone, Deserialize)]
pub struct RecoveryOptionDetail {
    pub action: String,
    pub field: Option<String>,
    pub suggested_value: Option<String>,
    pub reason: String,
    pub example: Option<String>,
}

/// An instance as returned by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceResponse {
    pub id: String,
    pub machine: String,
    pub state: String,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
    pub state_entered_at: String,
    pub trail_length: u64,
    pub version: u64,
}

/// Result of a TRANSITION command.
#[derive(Debug, Clone, Deserialize)]
pub struct TransitionResponse {
    pub from_state: String,
    pub to_state: String,
    pub instance: InstanceResponse,
}

/// Result of a DEFINE MACHINE command.
#[derive(Debug, Clone, Deserialize)]
pub struct DefineResult {
    pub action: String,
    pub warnings: Option<Vec<String>>,
}

/// Options for a transition.
#[derive(Debug, Default, Clone)]
pub struct TransitionOptions {
    pub with_data: Vec<(String, serde_json::Value)>,
    pub memo: Option<String>,
    pub as_actor: Option<String>,
    /// Actor role for role-based guards. Sets ACTOR to `{id: as_actor, role: as_role}`.
    pub as_role: Option<String>,
    /// Idempotency key for safe retries. If set, the server caches the result
    /// and returns it on subsequent calls with the same key.
    pub idempotency_key: Option<String>,
}

/// Machine info from GET /machines/{name}.
#[derive(Debug, Clone, Deserialize)]
pub struct MachineInfo {
    pub name: String,
    pub states: Vec<String>,
    pub initial_state: String,
    pub terminal_states: Vec<String>,
    pub version: u64,
}

/// A trail entry.
#[derive(Debug, Clone, Deserialize)]
pub struct TrailEntryResponse {
    pub sequence: u64,
    pub from_state: String,
    pub to_state: String,
    pub actor: Option<String>,
    pub memo: Option<String>,
    pub timestamp: String,
}

/// An event received via WebSocket subscription.
#[derive(Debug, Clone, Deserialize)]
pub struct SdkEvent {
    pub event: String,
    pub machine: String,
    #[serde(default)]
    pub instance_id: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transition_options_default() {
        let opts = TransitionOptions::default();
        assert!(opts.with_data.is_empty());
        assert!(opts.memo.is_none());
        assert!(opts.as_actor.is_none());
    }

    #[test]
    fn test_transition_options_clone() {
        let opts = TransitionOptions {
            with_data: vec![("key".to_string(), serde_json::json!(42))],
            memo: Some("memo".to_string()),
            as_actor: Some("admin".to_string()),
            as_role: None,
            idempotency_key: None,
        };
        let cloned = opts.clone();
        assert_eq!(cloned.with_data.len(), 1);
        assert_eq!(cloned.memo, Some("memo".to_string()));
        assert_eq!(cloned.as_actor, Some("admin".to_string()));
    }

    #[test]
    fn test_transition_options_debug() {
        let opts = TransitionOptions::default();
        let debug_str = format!("{:?}", opts);
        assert!(debug_str.contains("TransitionOptions"));
    }

    #[test]
    fn test_instance_response_deserialize() {
        let json = serde_json::json!({
            "id": "01ABCDEFGHIJKLMNOPQRSTUVWX",
            "machine": "test",
            "state": "open",
            "data": {"key": "value"},
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "state_entered_at": "2026-01-01T00:00:00Z",
            "trail_length": 1,
            "version": 1
        });
        let resp: InstanceResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.id, "01ABCDEFGHIJKLMNOPQRSTUVWX");
        assert_eq!(resp.machine, "test");
        assert_eq!(resp.state, "open");
        assert_eq!(resp.trail_length, 1);
        assert_eq!(resp.version, 1);
    }

    #[test]
    fn test_instance_response_serialize() {
        let resp = InstanceResponse {
            id: "01ABCDEFGHIJKLMNOPQRSTUVWX".to_string(),
            machine: "test".to_string(),
            state: "open".to_string(),
            data: serde_json::json!({}),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            state_entered_at: "2026-01-01T00:00:00Z".to_string(),
            trail_length: 0,
            version: 1,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["id"], "01ABCDEFGHIJKLMNOPQRSTUVWX");
        assert_eq!(json["state"], "open");
    }

    #[test]
    fn test_instance_response_clone() {
        let resp = InstanceResponse {
            id: "abc".to_string(),
            machine: "m".to_string(),
            state: "s".to_string(),
            data: serde_json::json!(null),
            created_at: "t".to_string(),
            updated_at: "t".to_string(),
            state_entered_at: "t".to_string(),
            trail_length: 0,
            version: 0,
        };
        let cloned = resp.clone();
        assert_eq!(cloned.id, "abc");
    }

    #[test]
    fn test_execute_response_deserialize() {
        let json = serde_json::json!({
            "success": true,
            "result": {"key": "value"},
            "error": null,
            "warnings": ["warn1"]
        });
        let resp: ExecuteResponse = serde_json::from_value(json).unwrap();
        assert!(resp.success);
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
        assert_eq!(resp.warnings.unwrap().len(), 1);
    }

    #[test]
    fn test_execute_response_error() {
        let json = serde_json::json!({
            "success": false,
            "result": null,
            "error": "something went wrong",
            "warnings": null
        });
        let resp: ExecuteResponse = serde_json::from_value(json).unwrap();
        assert!(!resp.success);
        assert_eq!(resp.error, Some("something went wrong".to_string()));
    }

    #[test]
    fn test_transition_response_deserialize() {
        let json = serde_json::json!({
            "from_state": "idle",
            "to_state": "running",
            "instance": {
                "id": "abc",
                "machine": "m",
                "state": "running",
                "data": {},
                "created_at": "t",
                "updated_at": "t",
                "state_entered_at": "t",
                "trail_length": 2,
                "version": 2
            }
        });
        let resp: TransitionResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.from_state, "idle");
        assert_eq!(resp.to_state, "running");
        assert_eq!(resp.instance.state, "running");
    }

    #[test]
    fn test_define_result_deserialize() {
        let json = serde_json::json!({
            "action": "machine_defined",
            "warnings": null
        });
        let resp: DefineResult = serde_json::from_value(json).unwrap();
        assert_eq!(resp.action, "machine_defined");
        assert!(resp.warnings.is_none());
    }

    #[test]
    fn test_machine_info_deserialize() {
        let json = serde_json::json!({
            "name": "counter",
            "states": ["idle", "running", "done"],
            "initial_state": "idle",
            "terminal_states": ["done"],
            "version": 1
        });
        let info: MachineInfo = serde_json::from_value(json).unwrap();
        assert_eq!(info.name, "counter");
        assert_eq!(info.states.len(), 3);
        assert_eq!(info.initial_state, "idle");
        assert_eq!(info.terminal_states, vec!["done"]);
        assert_eq!(info.version, 1);
    }

    #[test]
    fn test_trail_entry_response_deserialize() {
        let json = serde_json::json!({
            "sequence": 1,
            "from_state": "idle",
            "to_state": "running",
            "actor": "admin",
            "memo": "starting",
            "timestamp": "2026-01-01T00:00:00Z"
        });
        let entry: TrailEntryResponse = serde_json::from_value(json).unwrap();
        assert_eq!(entry.sequence, 1);
        assert_eq!(entry.from_state, "idle");
        assert_eq!(entry.to_state, "running");
        assert_eq!(entry.actor.as_deref(), Some("admin"));
        assert_eq!(entry.memo.as_deref(), Some("starting"));
    }

    #[test]
    fn test_trail_entry_response_no_optional_fields() {
        let json = serde_json::json!({
            "sequence": 0,
            "from_state": "",
            "to_state": "idle",
            "actor": null,
            "memo": null,
            "timestamp": "2026-01-01T00:00:00Z"
        });
        let entry: TrailEntryResponse = serde_json::from_value(json).unwrap();
        assert_eq!(entry.sequence, 0);
        assert!(entry.actor.is_none());
        assert!(entry.memo.is_none());
    }

    #[test]
    fn test_sdk_event_deserialize() {
        let json = serde_json::json!({
            "event": "spawned",
            "machine": "counter",
            "instance_id": "abc",
            "actor": "system",
            "data": {"key": "value"}
        });
        let event: SdkEvent = serde_json::from_value(json).unwrap();
        assert_eq!(event.event, "spawned");
        assert_eq!(event.machine, "counter");
        assert_eq!(event.instance_id.as_deref(), Some("abc"));
        assert_eq!(event.actor.as_deref(), Some("system"));
        assert!(event.data.is_some());
    }

    #[test]
    fn test_sdk_event_deserialize_minimal() {
        let json = serde_json::json!({
            "event": "tick",
            "machine": "timer"
        });
        let event: SdkEvent = serde_json::from_value(json).unwrap();
        assert_eq!(event.event, "tick");
        assert_eq!(event.machine, "timer");
        assert!(event.instance_id.is_none());
        assert!(event.actor.is_none());
        assert!(event.data.is_none());
    }

    #[test]
    fn test_sdk_event_clone() {
        let event = SdkEvent {
            event: "test".to_string(),
            machine: "m".to_string(),
            instance_id: None,
            actor: None,
            data: None,
        };
        let cloned = event.clone();
        assert_eq!(cloned.event, "test");
    }
}
