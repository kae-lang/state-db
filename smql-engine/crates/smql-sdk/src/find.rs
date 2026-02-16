use crate::client::SmqlClient;
use crate::error::{SdkError, SdkResult};
use crate::smql_fmt;
use crate::types::InstanceResponse;

/// Builder for FIND queries.
pub struct FindBuilder<'a> {
    client: &'a SmqlClient,
    machine: String,
    filter: Option<String>,
    sort: Vec<(String, String)>,
    limit: Option<u64>,
    offset: Option<u64>,
}

impl<'a> FindBuilder<'a> {
    pub(crate) fn new(client: &'a SmqlClient, machine: &str) -> Self {
        Self {
            client,
            machine: machine.to_string(),
            filter: None,
            sort: Vec::new(),
            limit: None,
            offset: None,
        }
    }

    /// Filter to instances in a specific state.
    pub fn in_state(mut self, state: &str) -> Self {
        self.filter = Some(format!("STATE IS {}", state));
        self
    }

    /// Filter to instances stuck in a state for longer than a duration.
    pub fn stuck_in(mut self, state: &str, duration: &str) -> Self {
        self.filter = Some(format!("STUCK IN {} FOR {}", state, duration));
        self
    }

    /// Add a raw WHERE clause expression.
    pub fn where_clause(mut self, expr: &str) -> Self {
        self.filter = Some(expr.to_string());
        self
    }

    /// Add a sort clause.
    pub fn sort_by(mut self, field: &str, direction: &str) -> Self {
        self.sort.push((field.to_string(), direction.to_string()));
        self
    }

    /// Limit results.
    pub fn limit(mut self, n: u64) -> Self {
        self.limit = Some(n);
        self
    }

    /// Offset results.
    pub fn offset(mut self, n: u64) -> Self {
        self.offset = Some(n);
        self
    }

    /// Execute the query and return matching instances.
    pub async fn execute(self) -> SdkResult<Vec<InstanceResponse>> {
        let smql = smql_fmt::format_find(
            &self.machine,
            self.filter.as_deref(),
            &self.sort,
            self.limit,
            self.offset,
        );
        let resp = self.client.execute(&smql).await?;
        let result = resp.result.unwrap_or_default();

        // Single instance (GET) or list of instances (FIND)
        if let Some(arr) = result["instances"].as_array() {
            let instances: Vec<InstanceResponse> = arr
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            Ok(instances)
        } else {
            Ok(Vec::new())
        }
    }

    /// Execute the query and return only the count.
    pub async fn count(self) -> SdkResult<u64> {
        let smql = smql_fmt::format_find(
            &self.machine,
            self.filter.as_deref(),
            &self.sort,
            self.limit,
            self.offset,
        );
        let resp = self.client.execute(&smql).await?;
        let result = resp.result.unwrap_or_default();
        Ok(result["count"].as_u64().unwrap_or(0))
    }
}

/// Builder for AGGREGATE queries.
pub struct AggregateBuilder<'a> {
    client: &'a SmqlClient,
    machine: String,
    measure: Option<String>,
    group_by: Option<String>,
}

impl<'a> AggregateBuilder<'a> {
    pub(crate) fn new(client: &'a SmqlClient, machine: &str) -> Self {
        Self {
            client,
            machine: machine.to_string(),
            measure: None,
            group_by: None,
        }
    }

    /// Set the measure function (e.g., "COUNT()", "AVG(field)").
    pub fn measure(mut self, m: &str) -> Self {
        self.measure = Some(m.to_string());
        self
    }

    /// Group by state.
    pub fn group_by_state(mut self) -> Self {
        self.group_by = Some("state".to_string());
        self
    }

    /// Group by a data field.
    pub fn group_by_field(mut self, field: &str) -> Self {
        self.group_by = Some(field.to_string());
        self
    }

    /// Execute the aggregate query.
    pub async fn execute(self) -> SdkResult<serde_json::Value> {
        let smql = smql_fmt::format_aggregate(
            &self.machine,
            self.measure.as_deref(),
            self.group_by.as_deref(),
        );
        let resp = self.client.execute(&smql).await?;
        resp.result.ok_or_else(|| {
            SdkError::Deserialize("Missing result in aggregate response".to_string())
        })
    }
}
