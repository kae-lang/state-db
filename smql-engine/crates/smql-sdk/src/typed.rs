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
        let data: M::Data = serde_json::from_value(resp.data)
            .map_err(|e| SdkError::Deserialize(e.to_string()))?;
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
    pub async fn spawn_typed<M: SmqlMachine>(
        &self,
        data: M::Data,
    ) -> SdkResult<TypedInstance<M>> {
        let json_data = serde_json::to_value(&data)
            .map_err(|e| SdkError::Deserialize(e.to_string()))?;
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
