use crate::client::SmqlClient;
use crate::error::{SdkError, SdkResult};
use crate::find::FindBuilder;
use crate::types::InstanceResponse;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Trait for a generated SMQL machine type.
pub trait SmqlMachine: Send + Sync + 'static {
    /// The machine name as registered in the catalog.
    const MACHINE_NAME: &'static str;
    /// The Rust type for instance data fields.
    type Data: Serialize + DeserializeOwned + Send + Sync;
    /// The Rust enum for states.
    type State: SmqlState;
}

/// Trait for a generated state enum.
pub trait SmqlState: Send + Sync + 'static + Sized {
    /// Parse a state name string.
    fn from_str(s: &str) -> SdkResult<Self>;
    /// Get the state name as a string.
    fn as_str(&self) -> &str;
    /// Whether this state is terminal.
    fn is_terminal(&self) -> bool;
}

/// A typed instance with deserialized data and state enum.
#[derive(Debug, Clone)]
pub struct TypedInstance<M: SmqlMachine> {
    pub id: String,
    pub state: M::State,
    pub data: M::Data,
    pub created_at: String,
    pub updated_at: String,
    pub state_entered_at: String,
    pub trail_length: u64,
    pub version: u64,
}

impl<M: SmqlMachine> TypedInstance<M> {
    /// Convert from an untyped InstanceResponse.
    pub fn from_response(resp: InstanceResponse) -> SdkResult<Self> {
        let state = M::State::from_str(&resp.state)?;
        let data: M::Data =
            serde_json::from_value(resp.data).map_err(|e| SdkError::Deserialize(e.to_string()))?;
        Ok(Self {
            id: resp.id,
            state,
            data,
            created_at: resp.created_at,
            updated_at: resp.updated_at,
            state_entered_at: resp.state_entered_at,
            trail_length: resp.trail_length,
            version: resp.version,
        })
    }
}

/// Extension methods on SmqlClient for typed operations.
impl SmqlClient {
    /// Spawn a typed instance.
    pub async fn spawn_typed<M: SmqlMachine>(&self, data: M::Data) -> SdkResult<TypedInstance<M>> {
        let json_data =
            serde_json::to_value(&data).map_err(|e| SdkError::Deserialize(e.to_string()))?;
        let resp = self.spawn(M::MACHINE_NAME, json_data).await?;
        TypedInstance::<M>::from_response(resp)
    }

    /// Build a typed FIND query.
    pub fn find_typed<M: SmqlMachine>(&self) -> TypedFindBuilder<'_, M> {
        TypedFindBuilder {
            inner: self.find(M::MACHINE_NAME),
            _marker: std::marker::PhantomData,
        }
    }
}

/// A typed wrapper around FindBuilder that deserializes results.
pub struct TypedFindBuilder<'a, M: SmqlMachine> {
    inner: FindBuilder<'a>,
    _marker: std::marker::PhantomData<M>,
}

impl<'a, M: SmqlMachine> TypedFindBuilder<'a, M> {
    /// Filter to instances in a specific state.
    pub fn in_state(mut self, state: &str) -> Self {
        self.inner = self.inner.in_state(state);
        self
    }

    /// Add a raw WHERE clause.
    pub fn where_clause(mut self, expr: &str) -> Self {
        self.inner = self.inner.where_clause(expr);
        self
    }

    /// Limit results.
    pub fn limit(mut self, n: u64) -> Self {
        self.inner = self.inner.limit(n);
        self
    }

    /// Execute and return typed instances.
    pub async fn execute(self) -> SdkResult<Vec<TypedInstance<M>>> {
        let instances = self.inner.execute().await?;
        instances
            .into_iter()
            .map(TypedInstance::<M>::from_response)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::SdkError;
    use crate::types::InstanceResponse;

    // Test helper: a minimal machine definition
    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    struct TestData {
        name: String,
    }

    #[derive(Debug, Clone)]
    enum TestState {
        Open,
        Closed,
    }

    impl SmqlState for TestState {
        fn from_str(s: &str) -> SdkResult<Self> {
            match s {
                "open" => Ok(Self::Open),
                "closed" => Ok(Self::Closed),
                other => Err(SdkError::Parse(format!("Unknown state: {}", other))),
            }
        }
        fn as_str(&self) -> &str {
            match self {
                Self::Open => "open",
                Self::Closed => "closed",
            }
        }
        fn is_terminal(&self) -> bool {
            matches!(self, Self::Closed)
        }
    }

    #[derive(Debug)]
    struct TestMachine;
    impl SmqlMachine for TestMachine {
        const MACHINE_NAME: &'static str = "test";
        type Data = TestData;
        type State = TestState;
    }

    fn make_response(state: &str, data: serde_json::Value) -> InstanceResponse {
        InstanceResponse {
            id: "01ABCDEFGHIJKLMNOPQRSTUVWX".to_string(),
            machine: "test".to_string(),
            state: state.to_string(),
            data,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            state_entered_at: "2026-01-01T00:00:00Z".to_string(),
            trail_length: 1,
            version: 1,
        }
    }

    #[test]
    fn test_from_response_success() {
        let resp = make_response("open", serde_json::json!({"name": "Alice"}));
        let typed = TypedInstance::<TestMachine>::from_response(resp).unwrap();
        assert_eq!(typed.id, "01ABCDEFGHIJKLMNOPQRSTUVWX");
        assert_eq!(typed.data.name, "Alice");
        assert!(matches!(typed.state, TestState::Open));
        assert_eq!(typed.version, 1);
        assert_eq!(typed.trail_length, 1);
    }

    #[test]
    fn test_from_response_invalid_state() {
        let resp = make_response("unknown_state", serde_json::json!({"name": "Bob"}));
        let err = TypedInstance::<TestMachine>::from_response(resp).unwrap_err();
        match err {
            SdkError::Parse(msg) => assert!(msg.contains("Unknown state")),
            other => panic!("Expected Parse error, got: {:?}", other),
        }
    }

    #[test]
    fn test_from_response_invalid_data() {
        // Data doesn't match TestData (missing "name" field)
        let resp = make_response("open", serde_json::json!({"wrong_field": 42}));
        let err = TypedInstance::<TestMachine>::from_response(resp).unwrap_err();
        match err {
            SdkError::Deserialize(msg) => assert!(msg.contains("missing field")),
            other => panic!("Expected Deserialize error, got: {:?}", other),
        }
    }

    #[test]
    fn test_from_response_completely_wrong_data() {
        // Data is not an object at all
        let resp = make_response("closed", serde_json::json!("not an object"));
        let err = TypedInstance::<TestMachine>::from_response(resp).unwrap_err();
        match err {
            SdkError::Deserialize(msg) => {
                assert!(!msg.is_empty());
            }
            other => panic!("Expected Deserialize error, got: {:?}", other),
        }
    }

    #[test]
    fn test_smql_state_methods() {
        let open = TestState::from_str("open").unwrap();
        assert_eq!(open.as_str(), "open");
        assert!(!open.is_terminal());

        let closed = TestState::from_str("closed").unwrap();
        assert_eq!(closed.as_str(), "closed");
        assert!(closed.is_terminal());
    }

    #[test]
    fn test_smql_machine_const() {
        assert_eq!(TestMachine::MACHINE_NAME, "test");
    }
}
