use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use smql_ast::command::{Command, Statement};
use smql_ast::query::Query;
use smql_ast::value::Value;
use smql_engine_core::query::QueryResult;
use smql_storage::Instance;
use std::collections::BTreeMap;

use crate::server::AppState;

/// Build the full API router.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/execute", post(execute_smql))
        .route("/machines", get(list_machines))
        .route("/machines/{name}", get(get_machine))
        .route("/instances/{id}", get(get_instance))
        .with_state(state)
}

// --- Health check ---

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

// --- Execute raw SMQL ---

#[derive(Deserialize)]
struct ExecuteRequest {
    smql: String,
}

#[derive(Serialize)]
struct ExecuteResponse {
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

async fn execute_command(
    state: &AppState,
    cmd: Command,
) -> (StatusCode, Json<ExecuteResponse>) {
    match cmd {
        Command::DefineMachine(def) => {
            match state.engine.catalog.register(def) {
                Ok(warnings) => {
                    let warns: Vec<String> = warnings.iter().map(|w| w.message.clone()).collect();
                    (
                        StatusCode::CREATED,
                        Json(ExecuteResponse {
                            success: true,
                            result: Some(serde_json::json!({ "action": "machine_defined" })),
                            error: None,
                            warnings: if warns.is_empty() {
                                None
                            } else {
                                Some(warns)
                            },
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
            }
        }

        Command::Spawn(spawn_cmd) => match state.engine.spawn(&spawn_cmd).await {
            Ok(result) => (
                StatusCode::CREATED,
                Json(ExecuteResponse {
                    success: true,
                    result: Some(instance_to_json(&result.instance)),
                    error: None,
                    warnings: None,
                }),
            ),
            Err(e) => error_response(e),
        },

        Command::Transition(t_cmd) => match state.engine.transition(&t_cmd).await {
            Ok(result) => (
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
            ),
            Err(e) => error_response(e),
        },

        Command::TryTransition(t_cmd) => match state.engine.try_transition(&t_cmd).await {
            Ok(Some(result)) => (
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
            ),
            Ok(None) => (
                StatusCode::OK,
                Json(ExecuteResponse {
                    success: true,
                    result: Some(serde_json::json!({
                        "transitioned": false,
                    })),
                    error: None,
                    warnings: None,
                }),
            ),
            Err(e) => error_response(e),
        },

        Command::BatchTransition(_) => (
            StatusCode::NOT_IMPLEMENTED,
            Json(ExecuteResponse {
                success: false,
                result: None,
                error: Some("BATCH TRANSITION not yet implemented".to_string()),
                warnings: None,
            }),
        ),

        Command::AlterMachine(_) => (
            StatusCode::NOT_IMPLEMENTED,
            Json(ExecuteResponse {
                success: false,
                result: None,
                error: Some("ALTER MACHINE not yet implemented".to_string()),
                warnings: None,
            }),
        ),
    }
}

async fn execute_query(
    state: &AppState,
    query: Query,
) -> (StatusCode, Json<ExecuteResponse>) {
    match state.engine.execute_query(&query).await {
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

async fn get_machine(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
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

async fn get_instance(
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

fn value_to_json(val: &Value) -> serde_json::Value {
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
        Value::List(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
        Value::Set(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
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
            serde_json::json!({
                "count": items.len(),
                "instances": items,
            })
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
