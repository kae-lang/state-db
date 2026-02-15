use smql_ast::query::*;
use smql_ast::types::AggregateFunction;
use smql_ast::value::Value;
use smql_ast::{SmqlError, SmqlResult};
use smql_storage::instance::{Filter, Instance, TrailEntry};
use std::collections::{BTreeMap, HashMap};

use crate::engine::Engine;
use crate::eval::{eval_guard, EvalContext};

fn query_type_label(query: &Query) -> &'static str {
    match query {
        Query::Get(_) => "GET",
        Query::Find(_) => "FIND",
        Query::Aggregate(_) => "AGGREGATE",
        Query::Trail(_) => "TRAIL",
        Query::Paths(_) => "PATHS",
        Query::Funnel(_) => "FUNNEL",
        Query::ComparePaths(_) => "COMPARE_PATHS",
    }
}

/// Query results in various formats.
#[derive(Debug, Clone)]
pub enum QueryResult {
    /// Single instance
    Instance(Instance),
    /// List of instances
    Instances(Vec<Instance>),
    /// Trail entries
    Trail(Vec<TrailEntry>),
    /// Aggregate results
    Aggregate(Vec<AggregateRow>),
    /// Path analysis results
    Paths(Vec<PathResult>),
    /// Funnel analysis results
    Funnel(FunnelResult),
}

/// A row in aggregate query results.
#[derive(Debug, Clone)]
pub struct AggregateRow {
    pub group_key: BTreeMap<String, Value>,
    pub measures: BTreeMap<String, Value>,
}

/// A path taken by instances through states.
#[derive(Debug, Clone)]
pub struct PathResult {
    pub path: Vec<String>,
    pub count: usize,
}

/// Funnel analysis results.
#[derive(Debug, Clone)]
pub struct FunnelResult {
    pub stages: Vec<FunnelStage>,
}

/// A single stage in a funnel.
#[derive(Debug, Clone)]
pub struct FunnelStage {
    pub state: String,
    pub count: usize,
    pub conversion_rate: f64,
}

impl Engine {
    /// Execute a query against the engine.
    #[tracing::instrument(skip(self, query), fields(query_type = %query_type_label(query)))]
    pub async fn execute_query(&self, query: &Query) -> SmqlResult<QueryResult> {
        match query {
            Query::Get(q) => self.execute_get(q).await,
            Query::Find(q) => self.execute_find(q).await,
            Query::Aggregate(q) => self.execute_aggregate(q).await,
            Query::Trail(q) => self.execute_trail(q).await,
            Query::Paths(q) => self.execute_paths(q).await,
            Query::Funnel(q) => self.execute_funnel(q).await,
            Query::ComparePaths(_) => {
                // Compare paths is a more complex variant of paths
                Err(SmqlError::QueryError {
                    message: "COMPARE PATHS not yet implemented".to_string(),
                    hint: None,
                })
            }
        }
    }

    /// GET Machine instance_id — retrieve a single instance.
    async fn execute_get(&self, query: &GetQuery) -> SmqlResult<QueryResult> {
        let id = smql_storage::InstanceId::from_string(&query.instance_id)
            .map_err(|_| SmqlError::not_found("Instance", &query.instance_id))?;

        let instance = self
            .storage
            .get_instance(&id)
            .await?
            .ok_or_else(|| SmqlError::not_found("Instance", &query.instance_id))?;

        if instance.machine != query.machine {
            return Err(SmqlError::not_found("Instance", &query.instance_id));
        }

        Ok(QueryResult::Instance(instance))
    }

    /// FIND Machine WHERE ... — search for instances.
    async fn execute_find(&self, query: &FindQuery) -> SmqlResult<QueryResult> {
        // First get all instances for the machine
        let filter = Filter {
            limit: query.limit.map(|l| l as usize),
            offset: query.offset.map(|o| o as usize),
            ..Default::default()
        };

        let mut instances = self
            .storage
            .find_instances(&query.machine, &filter)
            .await?;

        // Apply WHERE filter using expression evaluator
        if let Some(filter_expr) = &query.filter {
            instances.retain(|inst| {
                let ctx = EvalContext::new(inst.data.clone(), inst.state.clone());
                eval_guard(filter_expr, &ctx).unwrap_or(false)
            });
        }

        // Apply SORT
        if !query.sort.is_empty() {
            instances.sort_by(|a, b| {
                for sort in &query.sort {
                    let va = a.data.get(&sort.field).cloned().unwrap_or(Value::Null);
                    let vb = b.data.get(&sort.field).cloned().unwrap_or(Value::Null);
                    let cmp = compare_values_for_sort(&va, &vb);
                    let cmp = match sort.direction {
                        smql_ast::types::SortDirection::Asc => cmp,
                        smql_ast::types::SortDirection::Desc => cmp.reverse(),
                    };
                    if cmp != std::cmp::Ordering::Equal {
                        return cmp;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }

        // Apply limit/offset post-filter (if not already applied by storage)
        if query.filter.is_some() {
            if let Some(offset) = query.offset {
                let offset = offset as usize;
                if offset < instances.len() {
                    instances = instances.into_iter().skip(offset).collect();
                } else {
                    instances.clear();
                }
            }
            if let Some(limit) = query.limit {
                instances.truncate(limit as usize);
            }
        }

        Ok(QueryResult::Instances(instances))
    }

    /// AGGREGATE Machine MEASURE ... GROUP BY ...
    async fn execute_aggregate(&self, query: &AggregateQuery) -> SmqlResult<QueryResult> {
        let filter = Filter::default();
        let mut instances = self
            .storage
            .find_instances(&query.machine, &filter)
            .await?;

        // Apply WHERE filter
        if let Some(filter_expr) = &query.filter {
            instances.retain(|inst| {
                let ctx = EvalContext::new(inst.data.clone(), inst.state.clone());
                eval_guard(filter_expr, &ctx).unwrap_or(false)
            });
        }

        // Group instances
        let groups = group_instances(&instances, &query.group_by);

        // Compute measures for each group
        let mut rows = Vec::new();
        for (group_key, group_instances) in groups {
            let mut measures = BTreeMap::new();

            for measure in &query.measures {
                let alias = measure
                    .alias
                    .clone()
                    .unwrap_or_else(|| format!("{}", measure.function));

                let value = compute_aggregate(
                    &measure.function,
                    measure.field.as_deref(),
                    &group_instances,
                );
                measures.insert(alias, value);
            }

            rows.push(AggregateRow {
                group_key,
                measures,
            });
        }

        Ok(QueryResult::Aggregate(rows))
    }

    /// TRAIL OF instance_id — get transition history.
    async fn execute_trail(&self, query: &TrailQuery) -> SmqlResult<QueryResult> {
        let id = smql_storage::InstanceId::from_string(&query.instance_id)
            .map_err(|_| SmqlError::not_found("Instance", &query.instance_id))?;

        let mut entries = self.storage.get_trail(&id).await?;

        // Apply trail filters
        if let Some(filter) = &query.filter {
            if let Some(actor) = &filter.actor {
                entries.retain(|e| e.actor.as_ref() == Some(actor));
            }
            if let Some(from) = &filter.from_state {
                entries.retain(|e| e.from_state == *from);
            }
            if let Some(to) = &filter.to_state {
                entries.retain(|e| e.to_state == *to);
            }
        }

        Ok(QueryResult::Trail(entries))
    }

    /// PATHS FROM Machine — analyze state sequences.
    async fn execute_paths(&self, query: &PathsQuery) -> SmqlResult<QueryResult> {
        let filter = Filter::default();
        let instances = self
            .storage
            .find_instances(&query.machine, &filter)
            .await?;

        let mut path_counts: HashMap<Vec<String>, usize> = HashMap::new();

        for inst in &instances {
            let trail = self.storage.get_trail(&inst.id).await?;
            if trail.is_empty() {
                continue;
            }

            let path: Vec<String> = {
                let mut p = vec![trail[0].from_state.clone()];
                for entry in &trail {
                    if !entry.to_state.is_empty() {
                        p.push(entry.to_state.clone());
                    }
                }
                p
            };

            *path_counts.entry(path).or_insert(0) += 1;
        }

        let mut results: Vec<PathResult> = path_counts
            .into_iter()
            .map(|(path, count)| PathResult { path, count })
            .collect();

        results.sort_by(|a, b| b.count.cmp(&a.count));

        if let Some(limit) = query.limit {
            results.truncate(limit as usize);
        }

        Ok(QueryResult::Paths(results))
    }

    /// FUNNEL Machine THROUGH [states] — conversion analysis.
    async fn execute_funnel(&self, query: &FunnelQuery) -> SmqlResult<QueryResult> {
        let filter = Filter::default();
        let mut instances = self
            .storage
            .find_instances(&query.machine, &filter)
            .await?;

        if let Some(filter_expr) = &query.filter {
            instances.retain(|inst| {
                let ctx = EvalContext::new(inst.data.clone(), inst.state.clone());
                eval_guard(filter_expr, &ctx).unwrap_or(false)
            });
        }

        let total = instances.len();
        let mut stages = Vec::new();

        for state in &query.states {
            let count = count_instances_that_visited_state(&instances, state, &self.storage).await;
            let rate = if total > 0 {
                count as f64 / total as f64
            } else {
                0.0
            };

            stages.push(FunnelStage {
                state: state.clone(),
                count,
                conversion_rate: rate,
            });
        }

        Ok(QueryResult::Funnel(FunnelResult { stages }))
    }
}

/// Group instances by GROUP BY clauses.
fn group_instances<'a>(
    instances: &'a [Instance],
    group_by: &[GroupByClause],
) -> Vec<(BTreeMap<String, Value>, Vec<&'a Instance>)> {
    if group_by.is_empty() {
        // No grouping — single group with all instances
        return vec![(
            BTreeMap::new(),
            instances.iter().collect(),
        )];
    }

    let mut groups: HashMap<String, (BTreeMap<String, Value>, Vec<&'a Instance>)> = HashMap::new();

    for inst in instances {
        let mut key_parts = Vec::new();
        let mut key_map = BTreeMap::new();

        for clause in group_by {
            match clause {
                GroupByClause::Field(field) => {
                    let val = inst.data.get(field).cloned().unwrap_or(Value::Null);
                    key_parts.push(format!("{}={}", field, val));
                    key_map.insert(field.clone(), val);
                }
                GroupByClause::State => {
                    key_parts.push(format!("state={}", inst.state));
                    key_map
                        .insert("state".to_string(), Value::Text(inst.state.clone()));
                }
                GroupByClause::TimeBucket { field, interval } => {
                    let val = inst.data.get(field).cloned().unwrap_or(Value::Null);
                    key_parts.push(format!("{}[{}]={}", field, interval, val));
                    key_map.insert(format!("{}_{}", field, interval), val);
                }
            }
        }

        let key = key_parts.join("|");
        groups
            .entry(key)
            .or_insert_with(|| (key_map, Vec::new()))
            .1
            .push(inst);
    }

    groups.into_values().collect()
}

/// Compute an aggregate function over a set of instances.
fn compute_aggregate(
    func: &AggregateFunction,
    field: Option<&str>,
    instances: &[&Instance],
) -> Value {
    match func {
        AggregateFunction::Count => Value::Int(instances.len() as i64),

        AggregateFunction::Sum => {
            let field = match field {
                Some(f) => f,
                None => return Value::Null,
            };
            let mut sum = 0i64;
            let mut is_float = false;
            let mut fsum = 0.0f64;

            for inst in instances {
                match inst.data.get(field) {
                    Some(Value::Int(v)) => {
                        sum += v;
                        fsum += *v as f64;
                    }
                    Some(Value::Float(v)) => {
                        is_float = true;
                        fsum += v;
                    }
                    _ => {}
                }
            }

            if is_float {
                Value::Float(fsum)
            } else {
                Value::Int(sum)
            }
        }

        AggregateFunction::Avg => {
            let field = match field {
                Some(f) => f,
                None => return Value::Null,
            };
            let mut sum = 0.0f64;
            let mut count = 0usize;

            for inst in instances {
                match inst.data.get(field) {
                    Some(Value::Int(v)) => {
                        sum += *v as f64;
                        count += 1;
                    }
                    Some(Value::Float(v)) => {
                        sum += v;
                        count += 1;
                    }
                    _ => {}
                }
            }

            if count > 0 {
                Value::Float(sum / count as f64)
            } else {
                Value::Null
            }
        }

        AggregateFunction::Min => {
            let field = match field {
                Some(f) => f,
                None => return Value::Null,
            };
            instances
                .iter()
                .filter_map(|inst| inst.data.get(field))
                .min_by(|a, b| compare_values_for_sort(a, b))
                .cloned()
                .unwrap_or(Value::Null)
        }

        AggregateFunction::Max => {
            let field = match field {
                Some(f) => f,
                None => return Value::Null,
            };
            instances
                .iter()
                .filter_map(|inst| inst.data.get(field))
                .max_by(|a, b| compare_values_for_sort(a, b))
                .cloned()
                .unwrap_or(Value::Null)
        }

        AggregateFunction::Percentile(p) => {
            let field = match field {
                Some(f) => f,
                None => return Value::Null,
            };
            let mut values: Vec<f64> = instances
                .iter()
                .filter_map(|inst| match inst.data.get(field) {
                    Some(Value::Int(v)) => Some(*v as f64),
                    Some(Value::Float(v)) => Some(*v),
                    _ => None,
                })
                .collect();

            if values.is_empty() {
                return Value::Null;
            }

            values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let idx = ((p / 100.0) * (values.len() - 1) as f64).round() as usize;
            let idx = idx.min(values.len() - 1);
            Value::Float(values[idx])
        }
    }
}

/// Compare two Values for sorting purposes.
fn compare_values_for_sort(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => {
            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
        }
        (Value::Text(a), Value::Text(b)) => a.cmp(b),
        (Value::DateTime(a), Value::DateTime(b)) => a.cmp(b),
        (Value::Date(a), Value::Date(b)) => a.cmp(b),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
        (Value::Null, _) => std::cmp::Ordering::Less,
        (_, Value::Null) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}

/// Count how many instances have visited a given state (based on trail or current state).
async fn count_instances_that_visited_state(
    instances: &[Instance],
    state: &str,
    storage: &std::sync::Arc<dyn smql_storage::Storage>,
) -> usize {
    let mut count = 0;
    for inst in instances {
        if inst.state == state {
            count += 1;
            continue;
        }
        // Check trail
        if let Ok(trail) = storage.get_trail(&inst.id).await {
            if trail.iter().any(|e| e.to_state == state) {
                count += 1;
            }
        }
    }
    count
}
