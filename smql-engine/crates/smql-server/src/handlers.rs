use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use smql_ast::command::{Command, Statement};
use smql_ast::query;
use smql_ast::value::Value;
use smql_engine_core::query::QueryResult;
use smql_hooks::EventBus;
use smql_storage::Instance;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use crate::metrics::SmqlMetrics;
use crate::server::AppState;
use crate::websocket::{self, SubscribeParams};

/// Build the full API router.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/execute", post(execute_smql))
        .route("/machines", get(list_machines))
        .route("/machines/:name", get(get_machine))
        .route("/instances/:id", get(get_instance).delete(delete_instance))
        .route("/metrics", get(metrics_endpoint))
        .route("/subscribe", get(ws_subscribe))
        .with_state(state)
}

// --- Health check ---

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

// --- Prometheus metrics endpoint ---

async fn metrics_endpoint(State(state): State<AppState>) -> impl IntoResponse {
    let body = state.metrics.encode();
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
}

// --- WebSocket subscribe endpoint ---

async fn ws_subscribe(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    Query(params): Query<SubscribeParams>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| websocket::handle_ws(socket, state.event_bus, params))
}

// --- Execute raw SMQL ---

#[derive(Deserialize)]
struct ExecuteRequest {
    smql: String,
}

#[derive(Serialize)]
pub(crate) struct ExecuteResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warnings: Option<Vec<String>>,
}

async fn execute_smql(
    State(state): State<AppState>,
    Json(req): Json<ExecuteRequest>,
) -> impl IntoResponse {
    let statements = match smql_parser::parse(&req.smql) {
        Ok(stmts) => stmts,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ExecuteResponse {
                    success: false,
                    result: None,
                    error: Some(e.to_string()),
                    warnings: None,
                }),
            );
        }
    };

    let statement = match statements.into_iter().next() {
        Some(stmt) => stmt,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ExecuteResponse {
                    success: false,
                    result: None,
                    error: Some("Empty SMQL input".to_string()),
                    warnings: None,
                }),
            );
        }
    };

    match statement {
        Statement::Command(cmd) => execute_command(&state, cmd).await,
        Statement::Query(query) => execute_query(&state, query).await,
    }
}

async fn execute_command(state: &AppState, cmd: Command) -> (StatusCode, Json<ExecuteResponse>) {
    match cmd {
        Command::DefineMachine(def) => match state.engine.catalog.register(def) {
            Ok(warnings) => {
                let warns: Vec<String> = warnings.iter().map(|w| w.message.clone()).collect();
                (
                    StatusCode::CREATED,
                    Json(ExecuteResponse {
                        success: true,
                        result: Some(serde_json::json!({ "action": "machine_defined" })),
                        error: None,
                        warnings: if warns.is_empty() { None } else { Some(warns) },
                    }),
                )
            }
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(ExecuteResponse {
                    success: false,
                    result: None,
                    error: Some(e.to_string()),
                    warnings: None,
                }),
            ),
        },

        Command::DefinePolicy(policy) => {
            let name = policy.name.clone();
            state.engine.catalog.register_policy(policy);
            (
                StatusCode::CREATED,
                Json(ExecuteResponse {
                    success: true,
                    result: Some(serde_json::json!({ "action": "policy_defined", "name": name })),
                    error: None,
                    warnings: None,
                }),
            )
        }

        Command::DefineView(view) => {
            let name = view.name.clone();
            state.engine.catalog.register_view(view);
            (
                StatusCode::CREATED,
                Json(ExecuteResponse {
                    success: true,
                    result: Some(serde_json::json!({ "action": "view_defined", "name": name })),
                    error: None,
                    warnings: None,
                }),
            )
        }

        Command::DefineProjection(proj) => {
            let name = proj.name.clone();
            state.engine.catalog.register_projection(proj);
            (
                StatusCode::CREATED,
                Json(ExecuteResponse {
                    success: true,
                    result: Some(serde_json::json!({ "action": "projection_defined", "name": name })),
                    error: None,
                    warnings: None,
                }),
            )
        }

        Command::DefineRule(rule) => {
            let name = rule.name.clone();
            state.engine.catalog.register_rule(rule);
            (
                StatusCode::CREATED,
                Json(ExecuteResponse {
                    success: true,
                    result: Some(serde_json::json!({ "action": "rule_defined", "name": name })),
                    error: None,
                    warnings: None,
                }),
            )
        }

        Command::DefineSubscription(sub) => {
            let name = sub.name.clone();
            state.engine.catalog.register_subscription(sub);
            (
                StatusCode::CREATED,
                Json(ExecuteResponse {
                    success: true,
                    result: Some(serde_json::json!({ "action": "subscription_defined", "name": name })),
                    error: None,
                    warnings: None,
                }),
            )
        }

        Command::DefineSaga(saga) => {
            let name = saga.name.clone();
            state.engine.catalog.register_saga(saga);
            (
                StatusCode::CREATED,
                Json(ExecuteResponse {
                    success: true,
                    result: Some(serde_json::json!({ "action": "saga_defined", "name": name })),
                    error: None,
                    warnings: None,
                }),
            )
        }

        Command::Spawn(spawn_cmd) => {
            let machine_name = spawn_cmd.machine.clone();
            let start = Instant::now();
            match state.engine.spawn(&spawn_cmd).await {
                Ok(result) => {
                    let elapsed = start.elapsed().as_secs_f64();
                    // Record metrics
                    state
                        .metrics
                        .spawns_total
                        .with_label_values(&[&machine_name])
                        .inc();
                    state
                        .metrics
                        .instances_total
                        .with_label_values(&[&result.instance.machine, &result.instance.state])
                        .inc();
                    state
                        .metrics
                        .transition_duration_seconds
                        .with_label_values(&[&machine_name])
                        .observe(elapsed);

                    (
                        StatusCode::CREATED,
                        Json(ExecuteResponse {
                            success: true,
                            result: Some(instance_to_json(&result.instance)),
                            error: None,
                            warnings: None,
                        }),
                    )
                }
                Err(e) => error_response(e),
            }
        }

        Command::Transition(t_cmd) => {
            let machine_name = t_cmd.machine.clone();
            let start = Instant::now();
            match state.engine.transition(&t_cmd).await {
                Ok(result) => {
                    let elapsed = start.elapsed().as_secs_f64();
                    let machine = &result.instance.machine;
                    // Record metrics
                    state
                        .metrics
                        .transitions_total
                        .with_label_values(&[machine, &result.from_state, &result.to_state])
                        .inc();
                    state
                        .metrics
                        .transition_duration_seconds
                        .with_label_values(&[machine])
                        .observe(elapsed);
                    // Update instance gauge: decrement old state, increment new
                    state
                        .metrics
                        .instances_total
                        .with_label_values(&[machine, &result.from_state])
                        .dec();
                    state
                        .metrics
                        .instances_total
                        .with_label_values(&[machine, &result.to_state])
                        .inc();

                    (
                        StatusCode::OK,
                        Json(ExecuteResponse {
                            success: true,
                            result: Some(serde_json::json!({
                                "from_state": result.from_state,
                                "to_state": result.to_state,
                                "instance": instance_to_json(&result.instance),
                            })),
                            error: None,
                            warnings: None,
                        }),
                    )
                }
                Err(ref e) if matches!(e, smql_ast::SmqlError::TransitionDenied(_)) => {
                    state
                        .metrics
                        .guard_failures_total
                        .with_label_values(&[&machine_name])
                        .inc();
                    error_response(e.clone())
                }
                Err(e) => error_response(e),
            }
        }

        Command::TryTransition(t_cmd) => {
            let machine_name = t_cmd.machine.clone();
            let start = Instant::now();
            match state.engine.try_transition(&t_cmd).await {
                Ok(Some(result)) => {
                    let elapsed = start.elapsed().as_secs_f64();
                    let machine = &result.instance.machine;
                    state
                        .metrics
                        .transitions_total
                        .with_label_values(&[machine, &result.from_state, &result.to_state])
                        .inc();
                    state
                        .metrics
                        .transition_duration_seconds
                        .with_label_values(&[machine])
                        .observe(elapsed);
                    state
                        .metrics
                        .instances_total
                        .with_label_values(&[machine, &result.from_state])
                        .dec();
                    state
                        .metrics
                        .instances_total
                        .with_label_values(&[machine, &result.to_state])
                        .inc();

                    (
                        StatusCode::OK,
                        Json(ExecuteResponse {
                            success: true,
                            result: Some(serde_json::json!({
                                "transitioned": true,
                                "from_state": result.from_state,
                                "to_state": result.to_state,
                                "instance": instance_to_json(&result.instance),
                            })),
                            error: None,
                            warnings: None,
                        }),
                    )
                }
                Ok(None) => {
                    state
                        .metrics
                        .guard_failures_total
                        .with_label_values(&[&machine_name])
                        .inc();
                    (
                        StatusCode::OK,
                        Json(ExecuteResponse {
                            success: true,
                            result: Some(serde_json::json!({
                                "transitioned": false,
                            })),
                            error: None,
                            warnings: None,
                        }),
                    )
                }
                Err(e) => error_response(e),
            }
        }

        Command::BatchTransition(batch_cmd) => {
            let machine_name = batch_cmd.machine.clone();
            let to_state = batch_cmd.to_state.clone();
            let start = Instant::now();
            match state.engine.batch_transition(&batch_cmd).await {
                Ok(result) => {
                    let elapsed = start.elapsed().as_secs_f64();
                    state
                        .metrics
                        .transition_duration_seconds
                        .with_label_values(&[&machine_name])
                        .observe(elapsed);

                    // Update per-transition metrics
                    for (from_state, count) in &result.from_states {
                        for _ in 0..*count {
                            state
                                .metrics
                                .transitions_total
                                .with_label_values(&[&machine_name, from_state, &to_state])
                                .inc();
                            state
                                .metrics
                                .instances_total
                                .with_label_values(&[&machine_name, from_state])
                                .dec();
                            state
                                .metrics
                                .instances_total
                                .with_label_values(&[&machine_name, &to_state])
                                .inc();
                        }
                    }
                    // Track guard failures
                    for _ in &result.failures {
                        state
                            .metrics
                            .guard_failures_total
                            .with_label_values(&[&machine_name])
                            .inc();
                    }

                    let failures_json: Vec<serde_json::Value> = result
                        .failures
                        .iter()
                        .map(|f| {
                            serde_json::json!({
                                "instance_id": f.instance_id,
                                "error": f.error,
                            })
                        })
                        .collect();

                    (
                        StatusCode::OK,
                        Json(ExecuteResponse {
                            success: true,
                            result: Some(serde_json::json!({
                                "action": "batch_transition",
                                "machine": machine_name,
                                "matched": result.matched,
                                "transitioned": result.transitioned,
                                "failed": result.failures.len(),
                                "failures": failures_json,
                            })),
                            error: None,
                            warnings: None,
                        }),
                    )
                }
                Err(e) => error_response(e),
            }
        }

        Command::AlterMachine(alter_cmd) => {
            match state.engine.execute_alter_machine(&alter_cmd).await {
                Ok(result) => {
                    let warns: Vec<String> = result.warnings;
                    (
                        StatusCode::OK,
                        Json(ExecuteResponse {
                            success: true,
                            result: Some(serde_json::json!({
                                "action": "machine_altered",
                                "machine": result.machine,
                                "new_version": result.new_version,
                                "operations_applied": result.operations_applied,
                                "instances_migrated": result.instances_migrated,
                            })),
                            error: None,
                            warnings: if warns.is_empty() { None } else { Some(warns) },
                        }),
                    )
                }
                Err(e) => error_response(e),
            }
        }
    }
}

async fn execute_query(
    state: &AppState,
    query: query::Query,
) -> (StatusCode, Json<ExecuteResponse>) {
    let query_type = match &query {
        query::Query::Get(_) => "GET",
        query::Query::Find(_) => "FIND",
        query::Query::Trail(_) => "TRAIL",
        query::Query::Aggregate(_) => "AGGREGATE",
        query::Query::Paths(_) => "PATHS",
        query::Query::Funnel(_) => "FUNNEL",
        query::Query::ComparePaths(_) => "COMPARE_PATHS",
        query::Query::GetView(_) => "GET_VIEW",
        query::Query::GetProjection(_) => "GET_PROJECTION",
    };

    let start = Instant::now();
    let result = state.engine.execute_query(&query).await;
    let elapsed = start.elapsed().as_secs_f64();

    state
        .metrics
        .query_duration_seconds
        .with_label_values(&[query_type])
        .observe(elapsed);

    match result {
        Ok(result) => (
            StatusCode::OK,
            Json(ExecuteResponse {
                success: true,
                result: Some(query_result_to_json(result)),
                error: None,
                warnings: None,
            }),
        ),
        Err(e) => error_response(e),
    }
}

// --- REST endpoints ---

async fn list_machines(State(state): State<AppState>) -> impl IntoResponse {
    let names = state.engine.catalog.list();
    Json(serde_json::json!({ "machines": names }))
}

async fn get_machine(State(state): State<AppState>, Path(name): Path<String>) -> impl IntoResponse {
    match state.engine.catalog.get(&name) {
        Ok(def) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "name": def.name,
                "states": def.states.iter().map(|s| &s.name).collect::<Vec<_>>(),
                "initial_state": def.initial_state,
                "terminal_states": def.terminal_states,
                "version": def.version,
            })),
        ),
        Err(_) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("Machine '{}' not found", name) })),
        ),
    }
}

async fn get_instance(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    let instance_id = match smql_storage::InstanceId::from_string(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid instance ID" })),
            );
        }
    };

    match state.engine.storage.get_instance(&instance_id).await {
        Ok(Some(inst)) => (StatusCode::OK, Json(instance_to_json(&inst))),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("Instance '{}' not found", id) })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn delete_instance(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let instance_id = match smql_storage::InstanceId::from_string(&id) {
        Ok(id) => id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "Invalid instance ID" })),
            );
        }
    };

    // Check instance exists first (to return 404 if not)
    match state.engine.storage.get_instance(&instance_id).await {
        Ok(Some(inst)) => {
            match state.engine.storage.delete_instance(&instance_id).await {
                Ok(()) => {
                    // Update metrics: decrement instance gauge
                    state
                        .metrics
                        .instances_total
                        .with_label_values(&[&inst.machine, &inst.state])
                        .dec();
                    (
                        StatusCode::OK,
                        Json(serde_json::json!({
                            "deleted": true,
                            "id": id,
                        })),
                    )
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                ),
            }
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("Instance '{}' not found", id) })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

// --- Helpers ---

fn instance_to_json(inst: &Instance) -> serde_json::Value {
    let data: BTreeMap<String, serde_json::Value> = inst
        .data
        .iter()
        .map(|(k, v)| (k.clone(), value_to_json(v)))
        .collect();

    serde_json::json!({
        "id": inst.id.as_str(),
        "machine": inst.machine,
        "state": inst.state,
        "data": data,
        "created_at": inst.created_at.to_rfc3339(),
        "updated_at": inst.updated_at.to_rfc3339(),
        "state_entered_at": inst.state_entered_at.to_rfc3339(),
        "trail_length": inst.trail_length,
        "version": inst.version,
    })
}

pub fn value_to_json(val: &Value) -> serde_json::Value {
    match val {
        Value::Text(s) => serde_json::Value::String(s.clone()),
        Value::Int(v) => serde_json::json!(v),
        Value::Float(v) => serde_json::json!(v),
        Value::Bool(v) => serde_json::json!(v),
        Value::Null => serde_json::Value::Null,
        Value::Uuid(v) => serde_json::json!(v.to_string()),
        Value::DateTime(v) => serde_json::json!(v.to_rfc3339()),
        Value::Date(v) => serde_json::json!(v.to_string()),
        Value::Duration(d) => serde_json::json!(d.to_string()),
        Value::List(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Set(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Map(entries) => {
            let obj: serde_json::Map<String, serde_json::Value> = entries
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::Ref(machine, id) => serde_json::json!({ "ref": format!("{}#{}", machine, id) }),
        Value::Money(amount, currency) => serde_json::json!({
            "amount": amount,
            "currency": currency,
        }),
        Value::Blob(data) => serde_json::json!({ "blob_size": data.len() }),
        Value::Json(v) => v.clone(),
    }
}

fn query_result_to_json(result: QueryResult) -> serde_json::Value {
    match result {
        QueryResult::Instance(inst) => instance_to_json(&inst),
        QueryResult::Instances(insts) => {
            let items: Vec<serde_json::Value> = insts.iter().map(instance_to_json).collect();
            let next_cursor = insts.last().map(|inst| inst.id.as_str());
            let mut result = serde_json::json!({
                "count": items.len(),
                "instances": items,
            });
            if let Some(cursor) = next_cursor {
                result["next_cursor"] = serde_json::Value::String(cursor);
            }
            result
        }
        QueryResult::Trail(entries) => {
            let items: Vec<serde_json::Value> = entries
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "sequence": e.sequence,
                        "from_state": e.from_state,
                        "to_state": e.to_state,
                        "actor": e.actor,
                        "memo": e.memo,
                        "timestamp": e.timestamp.to_rfc3339(),
                    })
                })
                .collect();
            serde_json::json!({
                "count": items.len(),
                "entries": items,
            })
        }
        QueryResult::Aggregate(rows) => {
            let items: Vec<serde_json::Value> = rows
                .iter()
                .map(|r| {
                    let group: BTreeMap<String, serde_json::Value> = r
                        .group_key
                        .iter()
                        .map(|(k, v)| (k.clone(), value_to_json(v)))
                        .collect();
                    let measures: BTreeMap<String, serde_json::Value> = r
                        .measures
                        .iter()
                        .map(|(k, v)| (k.clone(), value_to_json(v)))
                        .collect();
                    serde_json::json!({
                        "group": group,
                        "measures": measures,
                    })
                })
                .collect();
            serde_json::json!({
                "rows": items,
            })
        }
        QueryResult::Paths(paths) => {
            let items: Vec<serde_json::Value> = paths
                .iter()
                .map(|p| {
                    serde_json::json!({
                        "path": p.path,
                        "count": p.count,
                    })
                })
                .collect();
            serde_json::json!({
                "paths": items,
            })
        }
        QueryResult::Funnel(funnel) => {
            let stages: Vec<serde_json::Value> = funnel
                .stages
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "state": s.state,
                        "count": s.count,
                        "conversion_rate": s.conversion_rate,
                    })
                })
                .collect();
            serde_json::json!({
                "stages": stages,
            })
        }
        QueryResult::ComparePaths(compare) => {
            let segments: Vec<serde_json::Value> = compare
                .segments
                .iter()
                .map(|seg| {
                    let paths: Vec<serde_json::Value> = seg
                        .paths
                        .iter()
                        .map(|p| {
                            serde_json::json!({
                                "path": p.path,
                                "count": p.count,
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "segment_value": value_to_json(&seg.segment_value),
                        "paths": paths,
                    })
                })
                .collect();
            serde_json::json!({
                "segment_by": compare.segment_by,
                "segments": segments,
            })
        }
    }
}

fn error_response(e: smql_ast::SmqlError) -> (StatusCode, Json<ExecuteResponse>) {
    let status = match &e {
        smql_ast::SmqlError::NotFound { .. } => StatusCode::NOT_FOUND,
        smql_ast::SmqlError::TransitionDenied(_) => StatusCode::CONFLICT,
        smql_ast::SmqlError::SpawnRejected { .. } => StatusCode::BAD_REQUEST,
        smql_ast::SmqlError::ValidationError { .. } => StatusCode::BAD_REQUEST,
        smql_ast::SmqlError::Conflict { .. } => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    (
        status,
        Json(ExecuteResponse {
            success: false,
            result: None,
            error: Some(e.to_string()),
            warnings: None,
        }),
    )
}

/// Test-only wrapper exposing error_response to the test module.
#[cfg(test)]
pub fn error_response_for_test(
    e: smql_ast::SmqlError,
) -> (StatusCode, Json<ExecuteResponse>) {
    error_response(e)
}

/// Start a background task that subscribes to EventBus and updates timeout metrics.
/// Timeout transitions are identified by actor = "System" in transition events.
pub fn start_event_metrics_listener(event_bus: Arc<EventBus>, metrics: Arc<SmqlMetrics>) {
    let mut receiver = event_bus.subscribe();
    tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    // Track timeout fires: events with name "TIMEOUT" are emitted by system
                    if event.name == "TIMEOUT" {
                        metrics
                            .timeout_fires_total
                            .with_label_values(&[&event.machine, ""])
                            .inc();
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "Metrics EventBus subscriber lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}
