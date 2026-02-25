use smql_ast::error::GuardFailure;
use smql_ast::query::*;
use smql_ast::types::AggregateFunction;
use smql_ast::value::Value;
use smql_ast::{SmqlError, SmqlResult};
use smql_storage::instance::{Filter, Instance, TrailEntry};
use std::collections::{BTreeMap, HashMap};

use crate::engine::Engine;
use crate::eval::{eval_guard, ActorInfo, EvalContext};

/// Hard upper bound on instances loaded for in-memory filtering/analytics.
/// Prevents OOM when FIND+WHERE or aggregate queries run on large datasets.
const MAX_QUERY_INSTANCES: usize = 100_000;

fn query_type_label(query: &Query) -> &'static str {
    match query {
        Query::Get(_) => "GET",
        Query::Find(_) => "FIND",
        Query::Aggregate(_) => "AGGREGATE",
        Query::Trail(_) => "TRAIL",
        Query::Paths(_) => "PATHS",
        Query::Funnel(_) => "FUNNEL",
        Query::ComparePaths(_) => "COMPARE_PATHS",
        Query::GetView(_) => "GET_VIEW",
        Query::GetProjection(_) => "GET_PROJECTION",
        Query::ExplainTransitions(_) => "EXPLAIN_TRANSITIONS",
        Query::GetEvents(_) => "GET_EVENTS",
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
    /// Segmented path comparison results
    ComparePaths(ComparePathsResult),
    /// Explain transitions result
    ExplainTransitions(ExplainTransitionsResult),
    /// Durable event log entries
    Events(Vec<smql_storage::instance::StoredEvent>),
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

/// Result of COMPARE PATHS — paths segmented by a data field.
#[derive(Debug, Clone)]
pub struct ComparePathsResult {
    pub segment_by: String,
    pub segments: Vec<PathSegment>,
}

/// One segment in a COMPARE PATHS result.
#[derive(Debug, Clone)]
pub struct PathSegment {
    pub segment_value: Value,
    pub paths: Vec<PathResult>,
}

/// Result of EXPLAIN TRANSITIONS — available transitions with guard evaluation.
#[derive(Debug, Clone)]
pub struct ExplainTransitionsResult {
    pub machine: String,
    pub current_state: Option<String>,
    pub instance_id: Option<String>,
    pub available: Vec<AvailableTransition>,
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
            Query::ComparePaths(q) => self.execute_compare_paths(q).await,
            Query::GetView(q) => self.execute_get_view(q).await,
            Query::GetProjection(q) => self.execute_get_projection(q).await,
            Query::ExplainTransitions(q) => self.execute_explain_transitions(q).await,
            Query::GetEvents(q) => self.execute_get_events(q).await,
        }
    }

    /// GET Machine instance_id [AS ACTOR role] — retrieve a single instance.
    async fn execute_get(&self, query: &GetQuery) -> SmqlResult<QueryResult> {
        let id = smql_storage::InstanceId::from_string(&query.instance_id)
            .map_err(|_| SmqlError::not_found("Instance", &query.instance_id))?;

        let mut instance = self
            .storage
            .get_instance(&id)
            .await?
            .ok_or_else(|| SmqlError::not_found("Instance", &query.instance_id))?;

        if instance.machine != query.machine {
            return Err(SmqlError::not_found("Instance", &query.instance_id));
        }

        // Check if instance is expired (TTL)
        if let Some(expires_at) = instance.expires_at {
            if chrono::Utc::now() > expires_at {
                return Err(SmqlError::ValidationError {
                    message: format!("Instance '{}' has expired", query.instance_id),
                    field: None,
                    hint: Some("This instance's TTL has elapsed. It is no longer accessible.".to_string()),
                });
            }
        }

        // Evaluate COMPUTED fields before returning
        if let Ok(machine_def) = self.catalog.get(&query.machine) {
            self.evaluate_computed_fields(&machine_def, &mut instance.data, &instance.state);
        }

        // Apply field-level read filtering if an actor role is specified
        if query.as_actor.is_some() {
            if let Ok(machine_def) = self.catalog.get(&query.machine) {
                instance.data = self.filter_readable_fields(
                    &machine_def,
                    &instance.data,
                    query.as_actor.as_deref(),
                );
            }
        }

        Ok(QueryResult::Instance(instance))
    }

    /// FIND Machine WHERE ... — search for instances.
    async fn execute_find(&self, query: &FindQuery) -> SmqlResult<QueryResult> {
        // When there's a WHERE filter, we must fetch instances first (no storage-level
        // offset/limit) so the filter sees every row. Offset/limit are applied after filtering.
        // Cap at MAX_QUERY_INSTANCES to prevent OOM on large datasets.
        let has_filter = query.filter.is_some();
        let filter = Filter {
            limit: if has_filter {
                Some(MAX_QUERY_INSTANCES)
            } else {
                query.limit.map(|l| l as usize)
            },
            offset: if has_filter { None } else { query.offset.map(|o| o as usize) },
            after_id: query.after.clone(),
            ..Default::default()
        };

        let mut instances = self.storage.find_instances(&query.machine, &filter).await?;

        // Filter out expired instances (TTL)
        let now = chrono::Utc::now();
        instances.retain(|inst| {
            inst.expires_at.map_or(true, |exp| now <= exp)
        });

        // Evaluate COMPUTED fields before filtering so WHERE can use computed values
        if let Ok(machine_def) = self.catalog.get(&query.machine) {
            for inst in &mut instances {
                self.evaluate_computed_fields(&machine_def, &mut inst.data, &inst.state);
            }
        }

        // Apply WHERE filter using expression evaluator
        if let Some(filter_expr) = &query.filter {
            // Look up terminal_states if filter uses ALIVE/TERMINATED predicates
            let terminal_states = if expr_uses_predicate(filter_expr, |k| {
                matches!(k, smql_ast::expression::ExpressionKind::Alive
                    | smql_ast::expression::ExpressionKind::Terminated)
            }) {
                self.catalog
                    .get(&query.machine)
                    .ok()
                    .map(|m| m.terminal_states.clone())
            } else {
                None
            };

            // Pre-load visited states for HAS_VISITED/NEVER_VISITED predicates
            let needs_trails = expr_uses_predicate(filter_expr, |k| {
                matches!(k, smql_ast::expression::ExpressionKind::HasVisited(_)
                    | smql_ast::expression::ExpressionKind::NeverVisited(_))
            });

            let trail_map: HashMap<String, std::collections::HashSet<String>> = if needs_trails {
                let mut map = HashMap::new();
                for inst in &instances {
                    let trail = self.storage.get_trail(&inst.id).await.unwrap_or_default();
                    let visited: std::collections::HashSet<String> = trail
                        .iter()
                        .flat_map(|t| {
                            let mut states = vec![t.to_state.clone()];
                            if !t.from_state.is_empty() {
                                states.push(t.from_state.clone());
                            }
                            states
                        })
                        .collect();
                    map.insert(inst.id.as_str(), visited);
                }
                map
            } else {
                HashMap::new()
            };

            instances.retain(|inst| {
                let mut ctx = EvalContext::new(inst.data.clone(), inst.state.clone());
                ctx.state_entered_at = inst.state_entered_at;
                ctx.created_at = inst.created_at;
                ctx.terminal_states = terminal_states.clone();
                ctx.tags = inst.tags.clone();
                let id_str = inst.id.as_str();
                if let Some(visited) = trail_map.get(&id_str) {
                    ctx.visited_states = Some(visited.clone());
                }
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

        // Apply offset/limit after filtering and sorting
        if has_filter {
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

        // Apply field-level read filtering if an actor role is specified
        if query.as_actor.is_some() {
            if let Ok(machine_def) = self.catalog.get(&query.machine) {
                for inst in &mut instances {
                    inst.data = self.filter_readable_fields(
                        &machine_def,
                        &inst.data,
                        query.as_actor.as_deref(),
                    );
                }
            }
        }

        // Apply field projection (SELECT)
        if let Some(ref select_fields) = query.select {
            for inst in &mut instances {
                inst.data.retain(|key, _| select_fields.contains(key));
            }
        }

        Ok(QueryResult::Instances(instances))
    }

    /// AGGREGATE Machine MEASURE ... GROUP BY ...
    async fn execute_aggregate(&self, query: &AggregateQuery) -> SmqlResult<QueryResult> {
        // Fast path: GROUP BY STATE with no filter and only COUNT measures (or no measures).
        // Uses count_by_state() which is O(S) instead of O(N) — avoids loading any instances.
        if query.filter.is_none()
            && query.group_by == [GroupByClause::State]
            && (query.measures.is_empty()
                || query.measures.iter().all(|m| {
                    matches!(m.function, AggregateFunction::Count) && m.field.is_none()
                }))
        {
            let counts = self.storage.count_by_state(&query.machine).await?;
            let mut rows = Vec::with_capacity(counts.len());
            for (state, count) in counts {
                let mut group_key = BTreeMap::new();
                group_key.insert("state".to_string(), Value::Text(state));
                let mut measures = BTreeMap::new();
                if query.measures.is_empty() {
                    measures.insert("count".to_string(), Value::Int(count as i64));
                } else {
                    for measure in &query.measures {
                        let alias = measure
                            .alias
                            .clone()
                            .unwrap_or_else(|| format!("{}", measure.function));
                        measures.insert(alias, Value::Int(count as i64));
                    }
                }
                rows.push(AggregateRow { group_key, measures });
            }
            return Ok(QueryResult::Aggregate(rows));
        }

        let filter = Filter { limit: Some(MAX_QUERY_INSTANCES), ..Default::default() };
        let mut instances = self.storage.find_instances(&query.machine, &filter).await?;

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
            // SINCE filter: keep only entries at or after the given timestamp
            if let Some(since_expr) = &filter.since {
                if let Some(since_ts) = eval_to_datetime(since_expr) {
                    entries.retain(|e| e.timestamp >= since_ts);
                }
            }
            // UNTIL filter: keep only entries at or before the given timestamp
            if let Some(until_expr) = &filter.until {
                if let Some(until_ts) = eval_to_datetime(until_expr) {
                    entries.retain(|e| e.timestamp <= until_ts);
                }
            }
        }

        Ok(QueryResult::Trail(entries))
    }

    /// PATHS FROM Machine — analyze state sequences.
    async fn execute_paths(&self, query: &PathsQuery) -> SmqlResult<QueryResult> {
        let filter = Filter { limit: Some(MAX_QUERY_INSTANCES), ..Default::default() };
        let mut instances = self.storage.find_instances(&query.machine, &filter).await?;

        // Apply WHERE filter
        if let Some(filter_expr) = &query.filter {
            instances.retain(|inst| {
                let ctx = EvalContext::new(inst.data.clone(), inst.state.clone());
                eval_guard(filter_expr, &ctx).unwrap_or(false)
            });
        }

        // Batch-load all trails at once instead of N+1 individual calls
        let ids: Vec<_> = instances.iter().map(|i| i.id.clone()).collect();
        let trails_map = self.storage.get_trails_batch(&ids).await?;

        let mut path_counts: HashMap<Vec<String>, usize> = HashMap::new();

        for inst in &instances {
            let trail = match trails_map.get(&inst.id.as_str()) {
                Some(t) => t,
                None => continue,
            };
            if trail.is_empty() {
                continue;
            }

            let path: Vec<String> = {
                let mut p = vec![trail[0].from_state.clone()];
                for entry in trail {
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
        let filter = Filter { limit: Some(MAX_QUERY_INSTANCES), ..Default::default() };
        let mut instances = self.storage.find_instances(&query.machine, &filter).await?;

        if let Some(filter_expr) = &query.filter {
            instances.retain(|inst| {
                let ctx = EvalContext::new(inst.data.clone(), inst.state.clone());
                eval_guard(filter_expr, &ctx).unwrap_or(false)
            });
        }

        // Batch-load all trails at once for funnel counting
        let ids: Vec<_> = instances.iter().map(|i| i.id.clone()).collect();
        let trails_map = self.storage.get_trails_batch(&ids).await?;

        let total = instances.len();
        let mut stages = Vec::new();

        for state in &query.states {
            let count = count_instances_that_visited_state_batch(&instances, state, &trails_map);
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

    /// COMPARE PATHS Machine SEGMENT BY field — segmented path analysis.
    async fn execute_compare_paths(&self, query: &ComparePathsQuery) -> SmqlResult<QueryResult> {
        let filter = Filter { limit: Some(MAX_QUERY_INSTANCES), ..Default::default() };
        let mut instances = self.storage.find_instances(&query.machine, &filter).await?;

        // Apply WHERE filter
        if let Some(filter_expr) = &query.filter {
            instances.retain(|inst| {
                let ctx = EvalContext::new(inst.data.clone(), inst.state.clone());
                eval_guard(filter_expr, &ctx).unwrap_or(false)
            });
        }

        // Batch-load all trails at once
        let ids: Vec<_> = instances.iter().map(|i| i.id.clone()).collect();
        let trails_map = self.storage.get_trails_batch(&ids).await?;

        // Group instances by segment_by field value, then count paths within each segment
        let mut segment_map: HashMap<String, (Value, HashMap<Vec<String>, usize>)> = HashMap::new();

        for inst in &instances {
            let segment_val = inst
                .data
                .get(&query.segment_by)
                .cloned()
                .unwrap_or(Value::Null);
            let segment_key = format!("{}", segment_val);

            let trail = match trails_map.get(&inst.id.as_str()) {
                Some(t) => t,
                None => continue,
            };
            if trail.is_empty() {
                continue;
            }

            let path: Vec<String> = {
                let mut p = vec![trail[0].from_state.clone()];
                for entry in trail {
                    if !entry.to_state.is_empty() {
                        p.push(entry.to_state.clone());
                    }
                }
                p
            };

            let entry = segment_map
                .entry(segment_key)
                .or_insert_with(|| (segment_val, HashMap::new()));
            *entry.1.entry(path).or_insert(0) += 1;
        }

        // Convert to result structs
        let mut segments: Vec<PathSegment> = segment_map
            .into_values()
            .map(|(segment_value, path_counts)| {
                let mut paths: Vec<PathResult> = path_counts
                    .into_iter()
                    .map(|(path, count)| PathResult { path, count })
                    .collect();
                paths.sort_by(|a, b| b.count.cmp(&a.count));
                PathSegment {
                    segment_value,
                    paths,
                }
            })
            .collect();

        // Sort segments by total count descending, then by segment value for determinism
        segments.sort_by(|a, b| {
            let total_a: usize = a.paths.iter().map(|p| p.count).sum();
            let total_b: usize = b.paths.iter().map(|p| p.count).sum();
            total_b.cmp(&total_a).then_with(|| {
                format!("{}", a.segment_value).cmp(&format!("{}", b.segment_value))
            })
        });

        Ok(QueryResult::ComparePaths(ComparePathsResult {
            segment_by: query.segment_by.clone(),
            segments,
        }))
    }

    /// GET VIEW name — execute a named view (runs its underlying FIND query).
    pub async fn execute_get_view(&self, query: &GetViewQuery) -> SmqlResult<QueryResult> {
        let view = self.catalog.get_view(&query.name).map_err(|_| {
            SmqlError::not_found("View", &query.name)
        })?;
        self.execute_find(&view.query).await
    }

    /// GET PROJECTION name — execute a named projection (runs its underlying AGGREGATE query).
    pub async fn execute_get_projection(&self, query: &GetProjectionQuery) -> SmqlResult<QueryResult> {
        let proj = self.catalog.get_projection_def(&query.name).map_err(|_| {
            SmqlError::not_found("Projection", &query.name)
        })?;
        self.execute_aggregate(&proj.query).await
    }

    /// EXPLAIN TRANSITIONS FOR Machine [instance_id] [AS actor]
    async fn execute_explain_transitions(
        &self,
        query: &ExplainTransitionsQuery,
    ) -> SmqlResult<QueryResult> {
        let machine_def = self
            .catalog
            .get(&query.machine)
            .map_err(|_| SmqlError::not_found("Machine", &query.machine))?;

        match &query.instance_id {
            None => {
                // Schema-level: return all transitions without guard evaluation
                let available = machine_def
                    .transitions
                    .iter()
                    .map(|t| {
                        let from_str = t.from.to_string();
                        let guard_strings: Vec<String> =
                            t.guards.iter().map(|g| g.to_string()).collect();
                        let requires_data = extract_field_refs_from_guards(&t.guards);
                        let requires_role = extract_actor_role_from_guards(&t.guards);
                        AvailableTransition {
                            from_state: from_str,
                            to_state: t.to.clone(),
                            guards: guard_strings,
                            guards_met: false, // Unknown without instance context
                            blocking_guards: vec![],
                            recovery_options: vec![],
                            requires_data,
                            requires_role,
                        }
                    })
                    .collect();

                Ok(QueryResult::ExplainTransitions(ExplainTransitionsResult {
                    machine: query.machine.clone(),
                    current_state: None,
                    instance_id: None,
                    available,
                }))
            }
            Some(instance_id) => {
                // Instance-level: evaluate guards against real data
                let id = smql_storage::InstanceId::from_string(instance_id)
                    .map_err(|_| SmqlError::not_found("Instance", instance_id))?;
                let instance = self
                    .storage
                    .get_instance(&id)
                    .await?
                    .ok_or_else(|| SmqlError::not_found("Instance", instance_id))?;

                if instance.machine != query.machine {
                    return Err(SmqlError::not_found("Instance", instance_id));
                }

                let current_state = instance.state.clone();
                let timeout_remaining = self
                    .timer_manager
                    .timeout_remaining(instance_id, &current_state);

                let mut ctx = EvalContext {
                    data: instance.data.clone(),
                    state: current_state.clone(),
                    actor: query.as_actor.as_ref().map(|a| ActorInfo {
                        id: a.clone(),
                        role: None,
                        capabilities: Vec::new(),
                        fields: HashMap::new(),
                    }),
                    state_entered_at: instance.state_entered_at,
                    created_at: instance.created_at,
                    now: chrono::Utc::now(),
                    timeout_remaining,
                    children: HashMap::new(),
                    parent_data: None,
                    parent_state: None,
                    terminal_states: None,
                    visited_states: None,
                    tags: instance.tags.clone(),
                };

                // Populate composition context if needed
                if !machine_def.children.is_empty() || instance.parent_id.is_some() {
                    self.populate_composition_context(&mut ctx, &instance, &machine_def)
                        .await;
                }

                // Find all transitions valid from the current state
                let mut available = Vec::new();
                for t in &machine_def.transitions {
                    let matches_source = match &t.from {
                        smql_ast::machine::TransitionSource::State(s) => s == &current_state,
                        smql_ast::machine::TransitionSource::Any { except } => {
                            !except.iter().any(|e| e == &current_state)
                        }
                        smql_ast::machine::TransitionSource::Group(_) => false,
                    };
                    if !matches_source {
                        continue;
                    }

                    let from_str = t.from.to_string();
                    let guard_strings: Vec<String> =
                        t.guards.iter().map(|g| g.to_string()).collect();
                    let requires_data = extract_field_refs_from_guards(&t.guards);
                    let requires_role = extract_actor_role_from_guards(&t.guards);

                    // Evaluate each guard
                    let mut blocking = Vec::new();
                    for guard in &t.guards {
                        match eval_guard(guard, &ctx) {
                            Ok(true) => {}
                            Ok(false) => {
                                blocking.push(GuardFailure {
                                    guard_expr: guard.to_string(),
                                    actual_value: None,
                                    expected: Some("true".to_string()),
                                    hint: None,
                                });
                            }
                            Err(e) => {
                                blocking.push(GuardFailure {
                                    guard_expr: guard.to_string(),
                                    actual_value: Some(e.to_string()),
                                    expected: None,
                                    hint: None,
                                });
                            }
                        }
                    }

                    // Also evaluate policy guards
                    for policy_name in &t.policies {
                        if let Ok(policy) = self.catalog.get_policy(policy_name) {
                            for guard in &policy.guards {
                                match eval_guard(guard, &ctx) {
                                    Ok(true) => {}
                                    Ok(false) => {
                                        blocking.push(GuardFailure {
                                            guard_expr: format!(
                                                "[POLICY {}] {}",
                                                policy_name, guard
                                            ),
                                            actual_value: None,
                                            expected: Some("true".to_string()),
                                            hint: Some(format!(
                                                "Guard from policy '{}'",
                                                policy_name
                                            )),
                                        });
                                    }
                                    Err(e) => {
                                        blocking.push(GuardFailure {
                                            guard_expr: format!(
                                                "[POLICY {}] {}",
                                                policy_name, guard
                                            ),
                                            actual_value: Some(e.to_string()),
                                            expected: None,
                                            hint: None,
                                        });
                                    }
                                }
                            }
                        }
                    }

                    let guards_met = blocking.is_empty();
                    let recovery_options = if !guards_met {
                        Engine::generate_recovery_options(
                            &blocking,
                            instance_id,
                            &query.machine,
                            &t.to,
                        )
                    } else {
                        vec![]
                    };

                    available.push(AvailableTransition {
                        from_state: from_str,
                        to_state: t.to.clone(),
                        guards: guard_strings,
                        guards_met,
                        blocking_guards: blocking,
                        recovery_options,
                        requires_data,
                        requires_role,
                    });
                }

                // Sort: guards_met=true first, then alphabetically by to_state
                available.sort_by(|a, b| {
                    b.guards_met
                        .cmp(&a.guards_met)
                        .then_with(|| a.to_state.cmp(&b.to_state))
                });

                Ok(QueryResult::ExplainTransitions(ExplainTransitionsResult {
                    machine: query.machine.clone(),
                    current_state: Some(current_state),
                    instance_id: Some(instance_id.clone()),
                    available,
                }))
            }
        }
    }
}

/// Extract data field names referenced in guard expressions.
fn extract_field_refs_from_guards(guards: &[smql_ast::Expression]) -> Vec<String> {
    let mut fields = Vec::new();
    for guard in guards {
        extract_field_refs_expr(&guard.kind, &mut fields);
    }
    fields.sort();
    fields.dedup();
    fields
}

fn extract_field_refs_expr(kind: &smql_ast::ExpressionKind, fields: &mut Vec<String>) {
    use smql_ast::ExpressionKind::*;
    match kind {
        FieldAccess(parts) => {
            if let Some(first) = parts.first() {
                fields.push(first.clone());
            }
        }
        QualifiedAccess { root, path: _ } => {
            extract_field_refs_expr(&root.kind, fields);
        }
        BinaryOp { left, op: _, right } => {
            extract_field_refs_expr(&left.kind, fields);
            extract_field_refs_expr(&right.kind, fields);
        }
        UnaryOp { op: _, operand } => {
            extract_field_refs_expr(&operand.kind, fields);
        }
        IsSet(inner) | IsNotSet(inner) => {
            extract_field_refs_expr(&inner.kind, fields);
        }
        FunctionCall { name: _, args } => {
            for arg in args {
                extract_field_refs_expr(&arg.kind, fields);
            }
        }
        InSet { expr, values } | InList { expr, values } => {
            extract_field_refs_expr(&expr.kind, fields);
            for v in values {
                extract_field_refs_expr(&v.kind, fields);
            }
        }
        InCollection { expr, collection } => {
            extract_field_refs_expr(&expr.kind, fields);
            extract_field_refs_expr(&collection.kind, fields);
        }
        All { collection, predicate } | Any { collection, predicate } => {
            extract_field_refs_expr(&collection.kind, fields);
            extract_field_refs_expr(&predicate.kind, fields);
        }
        Count(inner) => {
            if let Some(inner) = inner {
                extract_field_refs_expr(&inner.kind, fields);
            }
        }
        SignalFrom { machine: _, condition } => {
            extract_field_refs_expr(&condition.kind, fields);
        }
        Literal(_) | DurationLiteral(_) | SelfRef | ActorRef | StateIs(_) | StateIn(_)
        | Pattern(_) | Alive | Terminated | StuckIn { .. } | HasVisited(_)
        | NeverVisited(_) | TagEq { .. } => {}
    }
}

/// Check if any guard references ACTOR.role and extract the expected role value.
fn extract_actor_role_from_guards(guards: &[smql_ast::Expression]) -> Option<String> {
    for guard in guards {
        if let Some(role) = extract_actor_role_expr(&guard.kind) {
            return Some(role);
        }
    }
    None
}

fn extract_actor_role_expr(kind: &smql_ast::ExpressionKind) -> Option<String> {
    use smql_ast::ExpressionKind::*;
    match kind {
        QualifiedAccess { root, path } => {
            if matches!(root.kind, ActorRef)
                && path.len() == 1
                && path[0].eq_ignore_ascii_case("role")
            {
                return Some("*".to_string()); // Role is referenced but value unknown
            }
            None
        }
        BinaryOp { left, op: _, right } => {
            // Check for ACTOR.role == "value" pattern
            if let QualifiedAccess { root, path } = &left.kind {
                if matches!(root.kind, ActorRef)
                    && path.len() == 1
                    && path[0].eq_ignore_ascii_case("role")
                {
                    if let Literal(Value::Text(role_val)) = &right.kind {
                        return Some(role_val.clone());
                    }
                    return Some("*".to_string());
                }
            }
            if let QualifiedAccess { root, path } = &right.kind {
                if matches!(root.kind, ActorRef)
                    && path.len() == 1
                    && path[0].eq_ignore_ascii_case("role")
                {
                    if let Literal(Value::Text(role_val)) = &left.kind {
                        return Some(role_val.clone());
                    }
                    return Some("*".to_string());
                }
            }
            extract_actor_role_expr(&left.kind)
                .or_else(|| extract_actor_role_expr(&right.kind))
        }
        UnaryOp { op: _, operand } => extract_actor_role_expr(&operand.kind),
        _ => None,
    }
}

/// Group instances by GROUP BY clauses.
fn group_instances<'a>(
    instances: &'a [Instance],
    group_by: &[GroupByClause],
) -> Vec<(BTreeMap<String, Value>, Vec<&'a Instance>)> {
    if group_by.is_empty() {
        // No grouping — single group with all instances
        return vec![(BTreeMap::new(), instances.iter().collect())];
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
                    key_map.insert("state".to_string(), Value::Text(inst.state.clone()));
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
            let mut overflow = false;

            for inst in instances {
                match inst.data.get(field) {
                    Some(Value::Int(v)) => {
                        match sum.checked_add(*v) {
                            Some(s) => sum = s,
                            None => overflow = true,
                        }
                        fsum += *v as f64;
                    }
                    Some(Value::Float(v)) => {
                        is_float = true;
                        fsum += v;
                    }
                    _ => {}
                }
            }

            if is_float || overflow {
                // Fall back to float sum on integer overflow
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
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
        // Cross-type Int/Float comparison (matches eval.rs compare_values)
        (Value::Int(a), Value::Float(b)) => (*a as f64).partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
        (Value::Float(a), Value::Int(b)) => a.partial_cmp(&(*b as f64)).unwrap_or(std::cmp::Ordering::Equal),
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

/// Count how many instances have visited a given state using pre-loaded trails.
fn count_instances_that_visited_state_batch(
    instances: &[Instance],
    state: &str,
    trails_map: &HashMap<String, Vec<TrailEntry>>,
) -> usize {
    let mut count = 0;
    for inst in instances {
        if inst.state == state {
            count += 1;
            continue;
        }
        // Check pre-loaded trail
        if let Some(trail) = trails_map.get(&inst.id.as_str()) {
            if trail.iter().any(|e| e.to_state == state) {
                count += 1;
            }
        }
    }
    count
}

/// Check if an expression tree contains any node matching a predicate.
fn expr_uses_predicate(
    expr: &smql_ast::Expression,
    pred: impl Fn(&smql_ast::expression::ExpressionKind) -> bool + Copy,
) -> bool {
    use smql_ast::expression::ExpressionKind::*;
    if pred(&expr.kind) {
        return true;
    }
    match &expr.kind {
        BinaryOp { left, right, .. } => {
            expr_uses_predicate(left, pred) || expr_uses_predicate(right, pred)
        }
        UnaryOp { operand, .. } => expr_uses_predicate(operand, pred),
        IsSet(inner) | IsNotSet(inner) => expr_uses_predicate(inner, pred),
        FunctionCall { args, .. } => args.iter().any(|a| expr_uses_predicate(a, pred)),
        InSet { expr: e, values } | InList { expr: e, values } => {
            expr_uses_predicate(e, pred) || values.iter().any(|v| expr_uses_predicate(v, pred))
        }
        All { collection, predicate: p } | Any { collection, predicate: p } => {
            expr_uses_predicate(collection, pred) || expr_uses_predicate(p, pred)
        }
        Count(inner) => inner.as_ref().map_or(false, |i| expr_uses_predicate(i, pred)),
        SignalFrom { condition, .. } => expr_uses_predicate(condition, pred),
        QualifiedAccess { root, .. } => expr_uses_predicate(root, pred),
        _ => false,
    }
}

/// Evaluate a SINCE/UNTIL expression to a DateTime. Supports string literals (ISO 8601).
fn eval_to_datetime(expr: &smql_ast::Expression) -> Option<chrono::DateTime<chrono::Utc>> {
    use crate::eval::eval_expr;
    let ctx = EvalContext::new(HashMap::new(), String::new());
    match eval_expr(expr, &ctx) {
        Ok(Value::Text(s)) => {
            // Try parsing as ISO 8601 datetime
            chrono::DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .ok()
                .or_else(|| {
                    // Try parsing as date-only (YYYY-MM-DD)
                    chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                        .ok()
                        .and_then(|d| d.and_hms_opt(0, 0, 0))
                        .map(|dt| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc))
                })
        }
        Ok(Value::DateTime(dt)) => Some(dt),
        _ => None,
    }
}

impl Engine {
    /// GET EVENTS — retrieve durable event log entries.
    async fn execute_get_events(
        &self,
        query: &smql_ast::query::GetEventsQuery,
    ) -> SmqlResult<QueryResult> {
        let limit = query.limit.unwrap_or(100);
        let events = self
            .storage
            .get_events_after(
                query.after_id.as_deref(),
                query.machine.as_deref(),
                query.event_name.as_deref(),
                limit,
            )
            .await?;
        Ok(QueryResult::Events(events))
    }
}
