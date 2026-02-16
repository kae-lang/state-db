use serde::{Deserialize, Serialize};

/// Raw response from POST /execute.
#[derive(Debug, Deserialize)]
pub struct ExecuteResponse {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub warnings: Option<Vec<String>>,
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
