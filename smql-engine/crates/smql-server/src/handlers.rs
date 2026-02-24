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
use smql_engine_core::eval::ActorInfo;
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
        .route("/instances/:id/transitions", get(get_instance_transitions))
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
    // Sync event bus dropped count into prometheus metric
    let dropped = state.event_bus.dropped_count();
    let current = state.metrics.events_dropped_total.get();
    if dropped > current {
        state
            .metrics
            .events_dropped_total
            .inc_by(dropped - current);
    }
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
    /// Error metadata for agents: retryable flag and error category.
    #[serde(skip_serializing_if = "Option::is_none")]
    error_meta: Option<ErrorMeta>,
}

/// Metadata about an error to help agents decide how to respond.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ErrorMeta {
    /// Whether the operation can be retried (e.g., version conflicts, transient storage errors).
    pub retryable: bool,
    /// Error category: "conflict", "storage", "timeout", "internal", "validation".
    pub category: String,
}

async fn execute_smql(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    let (parts, body) = request.into_parts();

    // Extract optional Idempotency-Key header
    let idempotency_key = parts
        .headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Build actor override from JWT auth claims (when auth feature is enabled)
    #[cfg(feature = "auth")]
    let actor_override: Option<ActorInfo> = {
        let claims = parts.extensions.get::<crate::auth::AuthClaims>();
        let config = parts.extensions.get::<crate::auth::AuthConfig>();
        build_actor_from_auth(claims, config)
    };
    #[cfg(not(feature = "auth"))]
    let actor_override: Option<ActorInfo> = None;

    // Parse JSON body
    let body_bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ExecuteResponse {
                    success: false,
                    result: None,
                    error: Some("Request body too large".to_string()),
                    warnings: None,
                    error_meta: None,
                }),
            );
        }
    };
    let req: ExecuteRequest = match serde_json::from_slice(&body_bytes) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ExecuteResponse {
                    success: false,
                    result: None,
                    error: Some(format!("Invalid JSON: {}", e)),
                    warnings: None,
                    error_meta: None,
                }),
            );
        }
    };

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
                    error_meta: None,
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
                    error_meta: None,
                }),
            );
        }
    };

    match statement {
        Statement::Command(mut cmd) => {
            // Inject Idempotency-Key header into command if not already set via SMQL syntax
            if let Some(ref ikey) = idempotency_key {
                match &mut cmd {
                    Command::Spawn(ref mut spawn) if spawn.idempotency_key.is_none() => {
                        spawn.idempotency_key = Some(ikey.clone());
                    }
                    Command::Transition(ref mut t) | Command::TryTransition(ref mut t)
                        if t.idempotency_key.is_none() =>
                    {
                        t.idempotency_key = Some(ikey.clone());
                    }
                    _ => {}
                }
            }
            execute_command(&state, cmd, actor_override).await
        }
        Statement::Query(query) => execute_query(&state, query).await,
        Statement::Transaction(stmts) => {
            match state.engine.execute_transaction(&stmts).await {
                Ok(results) => {
                    let step_results: Vec<serde_json::Value> = results.iter().map(|r| {
                        serde_json::to_value(r).unwrap_or(serde_json::json!("unknown"))
                    }).collect();
                    (
                        StatusCode::OK,
                        Json(ExecuteResponse {
                            success: true,
                            result: Some(serde_json::json!({
                                "action": "transaction_committed",
                                "steps": step_results,
                            })),
                            error: None,
                            warnings: None,
                            error_meta: None,
                        }),
                    )
                }
                Err(e) => error_response(e),
            }
        }
    }
}

/// Build an `ActorInfo` override from JWT auth claims.
/// Returns `None` if no claims are present or if `trust_client_actor` is true.
#[cfg(feature = "auth")]
fn build_actor_from_auth(
    claims: Option<&crate::auth::AuthClaims>,
    config: Option<&crate::auth::AuthConfig>,
) -> Option<ActorInfo> {
    let claims = claims?;

    // If trust_client_actor is true, let the SMQL AS clause take precedence
    if config.map_or(false, |c| c.trust_client_actor) {
        return None;
    }

    Some(ActorInfo {
        id: claims.sub.clone(),
        role: claims.role.clone(),
        capabilities: claims.permissions.clone(),
        fields: std::collections::HashMap::new(),
    })
}

async fn execute_command(state: &AppState, cmd: Command, actor_override: Option<ActorInfo>) -> (StatusCode, Json<ExecuteResponse>) {
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
                        error_meta: None,
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
                    error_meta: None,
                }),
            ),
        },

        Command::DefineTemplate(def) => {
            let name = def.name.clone();
            state.engine.catalog.register_template(def);
            (
                StatusCode::CREATED,
                Json(ExecuteResponse {
                    success: true,
                    result: Some(serde_json::json!({ "action": "template_defined", "template": name })),
                    error: None,
                    warnings: None,
                    error_meta: None,
                }),
            )
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
                    error_meta: None,
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
                    error_meta: None,
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
                    error_meta: None,
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
                    error_meta: None,
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
                    error_meta: None,
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
                    error_meta: None,
                }),
            )
        }

        Command::Spawn(spawn_cmd) if spawn_cmd.batch => {
            let machine_name = spawn_cmd.machine.clone();
            match state.engine.batch_spawn(&spawn_cmd).await {
                Ok(result) => {
                    for _ in 0..result.created.len() {
                        state
                            .metrics
                            .spawns_total
                            .with_label_values(&[&machine_name])
                            .inc();
                    }
                    for inst in &result.created {
                        state
                            .metrics
                            .instances_total
                            .with_label_values(&[&inst.machine, &inst.state])
                            .inc();
                    }
                    let created_json: Vec<serde_json::Value> = result
                        .created
                        .iter()
                        .map(|inst| instance_to_json(inst))
                        .collect();
                    let failures_json: Vec<serde_json::Value> = result
                        .failures
                        .iter()
                        .map(|f| {
                            serde_json::json!({
                                "index": f.index,
                                "error": f.error,
                            })
                        })
                        .collect();
                    (
                        StatusCode::CREATED,
                        Json(ExecuteResponse {
                            success: result.failures.is_empty(),
                            result: Some(serde_json::json!({
                                "created": created_json,
                                "failures": failures_json,
                                "total_created": result.created.len(),
                                "total_failures": result.failures.len(),
                            })),
                            error: None,
                            warnings: None,
                            error_meta: None,
                        }),
                    )
                }
                Err(e) => error_response(e),
            }
        }

        Command::Spawn(spawn_cmd) => {
            let machine_name = spawn_cmd.machine.clone();
            let start = Instant::now();
            let spawn_result = if let Some(actor) = actor_override {
                state.engine.spawn_with_actor(&spawn_cmd, actor).await
            } else {
                state.engine.spawn(&spawn_cmd).await
            };
            match spawn_result {
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
                            error_meta: None,
                        }),
                    )
                }
                Err(e) => error_response(e),
            }
        }

        Command::Transition(t_cmd) => {
            let machine_name = t_cmd.machine.clone();
            let start = Instant::now();
            let transition_result = if let Some(actor) = actor_override {
                state.engine.transition_with_actor(&t_cmd, actor).await
            } else {
                state.engine.transition(&t_cmd).await
            };
            match transition_result {
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
                            error_meta: None,
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
            let try_result = if let Some(actor) = actor_override {
                state.engine.try_transition_with_actor(&t_cmd, actor).await
            } else {
                state.engine.try_transition(&t_cmd).await
            };
            match try_result {
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
                            error_meta: None,
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
                            error_meta: None,
                        }),
                    )
                }
                Err(e) => error_response(e),
            }
        }

        Command::BatchTransition(mut batch_cmd) => {
            // Override batch actor with JWT identity if present
            if let Some(ref actor) = actor_override {
                batch_cmd.as_actor = Some(actor.id.clone());
            }
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
                            error_meta: None,
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
                            error_meta: None,
                        }),
                    )
                }
                Err(e) => error_response(e),
            }
        }

        Command::Claim(claim_cmd) => {
            match state.engine.execute_claim(&claim_cmd).await {
                Ok(result) => (
                    StatusCode::OK,
                    Json(ExecuteResponse {
                        success: true,
                        result: Some(serde_json::json!({
                            "action": "instance_claimed",
                            "instance": instance_to_json(&result.instance),
                            "agent_id": result.agent_id,
                            "lease_expires_at": result.lease_expires_at.to_rfc3339(),
                        })),
                        error: None,
                        warnings: None,
                        error_meta: None,
                    }),
                ),
                Err(e) => error_response(e),
            }
        }

        Command::Release(release_cmd) => {
            match state.engine.execute_release(&release_cmd).await {
                Ok(result) => (
                    StatusCode::OK,
                    Json(ExecuteResponse {
                        success: true,
                        result: Some(serde_json::json!({
                            "action": "claim_released",
                            "instance_id": result.instance_id,
                            "agent_id": result.agent_id,
                        })),
                        error: None,
                        warnings: None,
                        error_meta: None,
                    }),
                ),
                Err(e) => error_response(e),
            }
        }
        Command::Watch(watch_cmd) => {
            match state.engine.watch(&watch_cmd).await {
                Ok(result) => (
                    StatusCode::OK,
                    Json(ExecuteResponse {
                        success: true,
                        result: Some(serde_json::json!({
                            "action": "watch_resolved",
                            "instance": instance_to_json(&result.instance),
                            "waited_ms": result.waited_ms,
                        })),
                        error: None,
                        warnings: None,
                        error_meta: None,
                    }),
                ),
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
        query::Query::ExplainTransitions(_) => "EXPLAIN_TRANSITIONS",
        query::Query::GetEvents(_) => "GET_EVENTS",
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
                error_meta: None,
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
        Err(e) => {
            tracing::error!("get_instance storage error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal server error" })),
            )
        }
    }
}

/// GET /instances/:id/transitions — explain available transitions for an instance.
async fn get_instance_transitions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<TransitionsQueryParams>,
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

    // Look up the instance to get its machine name
    let instance = match state.engine.storage.get_instance(&instance_id).await {
        Ok(Some(inst)) => inst,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": format!("Instance '{}' not found", id) })),
            );
        }
        Err(e) => {
            tracing::error!("get_instance_transitions storage error: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal server error" })),
            );
        }
    };

    let explain_query = smql_ast::query::ExplainTransitionsQuery {
        machine: instance.machine.clone(),
        instance_id: Some(id.clone()),
        as_actor: params.r#as.clone(),
    };

    match state
        .engine
        .execute_query(&smql_ast::query::Query::ExplainTransitions(explain_query))
        .await
    {
        Ok(result) => (StatusCode::OK, Json(query_result_to_json(result))),
        Err(e) => {
            tracing::error!("explain_transitions error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal server error" })),
            )
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct TransitionsQueryParams {
    /// Actor ID for guard evaluation context.
    r#as: Option<String>,
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
                Err(e) => {
                    tracing::error!("delete_instance storage error: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({ "error": "Internal server error" })),
                    )
                }
            }
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": format!("Instance '{}' not found", id) })),
        ),
        Err(e) => {
            tracing::error!("get_instance storage error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal server error" })),
            )
        }
    }
}

// --- Helpers ---

fn instance_to_json(inst: &Instance) -> serde_json::Value {
    let data: BTreeMap<String, serde_json::Value> = inst
        .data
        .iter()
        .map(|(k, v)| (k.clone(), value_to_json(v)))
        .collect();

    let mut json = serde_json::json!({
        "id": inst.id.as_str(),
        "machine": inst.machine,
        "state": inst.state,
        "data": data,
        "created_at": inst.created_at.to_rfc3339(),
        "updated_at": inst.updated_at.to_rfc3339(),
        "state_entered_at": inst.state_entered_at.to_rfc3339(),
        "trail_length": inst.trail_length,
        "version": inst.version,
    });
    if !inst.tags.is_empty() {
        json["tags"] = serde_json::json!(inst.tags);
    }
    if let Some(ref agent) = inst.claimed_by {
        json["claimed_by"] = serde_json::json!(agent);
    }
    if let Some(ref expires) = inst.claim_expires_at {
        json["claim_expires_at"] = serde_json::json!(expires.to_rfc3339());
    }
    if let Some(ref expires) = inst.expires_at {
        json["expires_at"] = serde_json::json!(expires.to_rfc3339());
    }
    json
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
        QueryResult::ExplainTransitions(explain) => {
            let transitions: Vec<serde_json::Value> = explain
                .available
                .iter()
                .map(|t| {
                    let blocking: Vec<serde_json::Value> = t
                        .blocking_guards
                        .iter()
                        .map(|g| {
                            serde_json::json!({
                                "guard_expr": g.guard_expr,
                                "actual_value": g.actual_value,
                                "expected": g.expected,
                                "hint": g.hint,
                            })
                        })
                        .collect();
                    let recovery: Vec<serde_json::Value> = t
                        .recovery_options
                        .iter()
                        .map(|r| {
                            serde_json::json!({
                                "action": format!("{:?}", r.action),
                                "field": r.field,
                                "suggested_value": r.suggested_value,
                                "reason": r.reason,
                                "example": r.example,
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "from_state": t.from_state,
                        "to_state": t.to_state,
                        "guards": t.guards,
                        "guards_met": t.guards_met,
                        "blocking_guards": blocking,
                        "recovery_options": recovery,
                        "requires_data": t.requires_data,
                        "requires_role": t.requires_role,
                    })
                })
                .collect();
            serde_json::json!({
                "machine": explain.machine,
                "current_state": explain.current_state,
                "instance_id": explain.instance_id,
                "transitions": transitions,
            })
        }
        QueryResult::Events(events) => {
            let items: Vec<serde_json::Value> = events
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "id": e.id,
                        "timestamp": e.timestamp.to_rfc3339(),
                        "machine": e.machine,
                        "event_name": e.event_name,
                        "instance_id": e.instance_id,
                        "payload": e.payload,
                        "actor": e.actor,
                    })
                })
                .collect();
            let next_cursor = events.last().map(|e| e.id.clone());
            let mut result = serde_json::json!({
                "count": items.len(),
                "events": items,
            });
            if let Some(cursor) = next_cursor {
                result["next_cursor"] = serde_json::Value::String(cursor);
            }
            result
        }
    }
}

fn error_response(e: smql_ast::SmqlError) -> (StatusCode, Json<ExecuteResponse>) {
    let (status, client_message, error_meta) = match &e {
        smql_ast::SmqlError::NotFound { .. } => (StatusCode::NOT_FOUND, e.to_string(), None),
        smql_ast::SmqlError::TransitionDenied(ref tde) => {
            // Include the full structured TransitionDeniedError as JSON in the result field
            // so SDK clients can extract recovery_options, guard_failures, llm_prompt, etc.
            let detail = serde_json::to_value(tde).ok();
            return (
                StatusCode::CONFLICT,
                Json(ExecuteResponse {
                    success: false,
                    result: detail,
                    error: Some(e.to_string()),
                    warnings: None,
                    error_meta: None,
                }),
            );
        }
        smql_ast::SmqlError::SpawnRejected { .. } => (StatusCode::BAD_REQUEST, e.to_string(), None),
        smql_ast::SmqlError::ValidationError { .. } => {
            (StatusCode::BAD_REQUEST, e.to_string(), None)
        }
        smql_ast::SmqlError::ParseError { .. } => (StatusCode::BAD_REQUEST, e.to_string(), None),
        smql_ast::SmqlError::Conflict { .. } => {
            // Version conflicts are retryable
            (
                StatusCode::CONFLICT,
                e.to_string(),
                Some(ErrorMeta {
                    retryable: true,
                    category: "conflict".to_string(),
                }),
            )
        }
        smql_ast::SmqlError::StorageError { retryable, .. } => {
            tracing::error!("Storage error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
                Some(ErrorMeta {
                    retryable: *retryable,
                    category: "storage".to_string(),
                }),
            )
        }
        smql_ast::SmqlError::TimeoutError { .. } => (
            StatusCode::REQUEST_TIMEOUT,
            e.to_string(),
            Some(ErrorMeta {
                retryable: true,
                category: "timeout".to_string(),
            }),
        ),
        _ => {
            // Don't leak internal errors to clients
            tracing::error!("Internal error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error".to_string(),
                Some(ErrorMeta {
                    retryable: false,
                    category: "internal".to_string(),
                }),
            )
        }
    };

    (
        status,
        Json(ExecuteResponse {
            success: false,
            result: None,
            error: Some(client_message),
            warnings: None,
            error_meta,
        }),
    )
}

/// Test-only wrapper exposing error_response to the test module.
#[cfg(test)]
pub(crate) fn error_response_for_test(
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
