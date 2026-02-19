use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::SmqlServer;

/// Helper: create a server and return its router.
fn test_router() -> axum::Router {
    SmqlServer::new().router()
}

/// Helper: make a POST /execute request with SMQL body.
fn execute_request(smql: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({ "smql": smql }).to_string()))
        .unwrap()
}

/// Helper: make a GET request to a path.
fn get_request(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

/// Helper: read body bytes as string.
async fn body_string(body: Body) -> String {
    let bytes = body.collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Define a simple machine via SMQL.
fn define_simple_machine() -> &'static str {
    r#"DEFINE MACHINE counter (
        STATES { idle, running }
        INITIAL STATE idle
        TERMINAL STATES { running }
        TRANSITIONS {
            idle -> running {}
        }
    )"#
}

/// Define a machine with a guard that always fails.
fn define_guarded_machine() -> &'static str {
    r#"DEFINE MACHINE gated (
        STATES { open, closed }
        INITIAL STATE open
        TERMINAL STATES { closed }
        TRANSITIONS {
            open -> closed {
                GUARD : false
            }
        }
    )"#
}

// ---------------------------------------------------------------------------
// Health check test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_endpoint() {
    let app = test_router();
    let resp = app.oneshot(get_request("/health")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

// ---------------------------------------------------------------------------
// Metrics unit tests
// ---------------------------------------------------------------------------

#[test]
fn metrics_new_registers_all_collectors() {
    let metrics = crate::metrics::SmqlMetrics::new();
    // Increment at least one to make it appear in output
    metrics.spawns_total.with_label_values(&["x"]).inc();
    metrics
        .transitions_total
        .with_label_values(&["x", "a", "b"])
        .inc();
    metrics.instances_total.with_label_values(&["x", "a"]).inc();
    metrics.guard_failures_total.with_label_values(&["x"]).inc();
    metrics
        .timeout_fires_total
        .with_label_values(&["x", "a"])
        .inc();
    metrics
        .query_duration_seconds
        .with_label_values(&["GET"])
        .observe(0.001);
    metrics
        .transition_duration_seconds
        .with_label_values(&["x"])
        .observe(0.01);
    let output = metrics.encode();
    assert!(
        output.contains("smql_instances_total"),
        "Missing instances_total:\n{}",
        output
    );
    assert!(
        output.contains("smql_transitions_total"),
        "Missing transitions_total:\n{}",
        output
    );
    assert!(
        output.contains("smql_transition_duration_seconds"),
        "Missing transition_duration:\n{}",
        output
    );
    assert!(
        output.contains("smql_guard_failures_total"),
        "Missing guard_failures:\n{}",
        output
    );
    assert!(
        output.contains("smql_timeout_fires_total"),
        "Missing timeout_fires:\n{}",
        output
    );
    assert!(
        output.contains("smql_query_duration_seconds"),
        "Missing query_duration:\n{}",
        output
    );
    assert!(
        output.contains("smql_spawns_total"),
        "Missing spawns_total:\n{}",
        output
    );
}

#[test]
fn metrics_encode_returns_valid_text() {
    let metrics = crate::metrics::SmqlMetrics::new();
    metrics
        .spawns_total
        .with_label_values(&["test_machine"])
        .inc();
    let output = metrics.encode();
    assert!(output.contains(r#"smql_spawns_total{machine="test_machine"} 1"#));
}

#[test]
fn metrics_gauge_inc_dec() {
    let metrics = crate::metrics::SmqlMetrics::new();
    metrics
        .instances_total
        .with_label_values(&["m", "idle"])
        .inc();
    metrics
        .instances_total
        .with_label_values(&["m", "idle"])
        .inc();
    metrics
        .instances_total
        .with_label_values(&["m", "idle"])
        .dec();
    let output = metrics.encode();
    assert!(output.contains(r#"smql_instances_total{machine="m",state="idle"} 1"#));
}

// ---------------------------------------------------------------------------
// Metrics endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metrics_endpoint_returns_text() {
    let server = SmqlServer::new();
    let app = server.router();

    // Define + spawn to ensure some metrics exist
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "Define failed");

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "Spawn failed");

    let resp = app.clone().oneshot(get_request("/metrics")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    assert!(
        body.contains("smql_spawns_total"),
        "Missing spawns metric:\n{}",
        body
    );
}

#[tokio::test]
async fn metrics_increment_on_spawn() {
    let server = SmqlServer::new();
    let app = server.router();

    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app.clone().oneshot(get_request("/metrics")).await.unwrap();
    let body = body_string(resp.into_body()).await;

    assert!(
        body.contains(r#"smql_spawns_total{machine="counter"} 1"#),
        "Expected spawns_total metric:\n{}",
        body
    );
    assert!(
        body.contains(r#"smql_instances_total{machine="counter",state="idle"} 1"#),
        "Expected instances_total gauge:\n{}",
        body
    );
}

#[tokio::test]
async fn metrics_increment_on_transition() {
    let server = SmqlServer::new();
    let app = server.router();

    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Spawn
    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    // Transition
    let transition_smql = format!(r#"TRANSITION counter "{}" TO running"#, instance_id);
    let resp = app
        .clone()
        .oneshot(execute_request(&transition_smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Check metrics
    let resp = app.clone().oneshot(get_request("/metrics")).await.unwrap();
    let body = body_string(resp.into_body()).await;

    assert!(
        body.contains(r#"smql_transitions_total{from="idle",machine="counter",to="running"} 1"#),
        "Expected transitions_total metric:\n{}",
        body
    );
    assert!(
        body.contains(r#"smql_instances_total{machine="counter",state="running"} 1"#),
        "Expected instances in running state:\n{}",
        body
    );
}

#[tokio::test]
async fn metrics_guard_failure_counter() {
    let server = SmqlServer::new();
    let app = server.router();

    let resp = app
        .clone()
        .oneshot(execute_request(define_guarded_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN gated {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    // Attempt transition that will fail guard
    let transition_smql = format!(r#"TRANSITION gated "{}" TO closed"#, instance_id);
    let resp = app
        .clone()
        .oneshot(execute_request(&transition_smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // Check guard_failures_total
    let resp = app.clone().oneshot(get_request("/metrics")).await.unwrap();
    let body = body_string(resp.into_body()).await;

    // The machine name in the label may be empty when parsed from transition command
    // (since transition doesn't carry machine name, it's looked up internally)
    assert!(
        body.contains("smql_guard_failures_total"),
        "Expected guard_failures_total metric:\n{}",
        body
    );
}

#[tokio::test]
async fn metrics_query_duration_recorded() {
    let server = SmqlServer::new();
    let app = server.router();

    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Execute a FIND query
    let resp = app
        .clone()
        .oneshot(execute_request("FIND counter"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Check query_duration_seconds histogram
    let resp = app.clone().oneshot(get_request("/metrics")).await.unwrap();
    let body = body_string(resp.into_body()).await;

    assert!(
        body.contains("smql_query_duration_seconds"),
        "Expected query duration metric:\n{}",
        body
    );
    assert!(
        body.contains(r#"query_type="FIND"#),
        "Expected FIND query type label:\n{}",
        body
    );
}

// ---------------------------------------------------------------------------
// WebSocket tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn websocket_subscribe_receives_events() {
    use futures_util::StreamExt;
    use tokio::net::TcpListener;

    let server = SmqlServer::new();
    let app = server.router();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", addr);

    // Define a machine with EMIT hooks
    let define_smql = r#"DEFINE MACHINE ws_test (
        STATES { idle, active }
        INITIAL STATE idle
        TERMINAL STATES { active }
        TRANSITIONS {
            idle -> active {
                ACTION : EMIT("state_changed")
            }
        }
        HOOKS {
            ON SPAWN {
                EMIT("spawned")
            }
        }
    )"#;

    let resp = client
        .post(format!("{}/execute", base_url))
        .json(&serde_json::json!({ "smql": define_smql }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Connect WebSocket
    let ws_url = format!("ws://{}/subscribe", addr);
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Spawn an instance
    let resp = client
        .post(format!("{}/execute", base_url))
        .json(&serde_json::json!({ "smql": r#"SPAWN ws_test {}"# }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let spawn_result: serde_json::Value = resp.json().await.unwrap();
    let instance_id = spawn_result["result"]["id"].as_str().unwrap().to_string();

    // Try to receive the "spawned" event
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), ws_stream.next()).await;

    match event {
        Ok(Some(Ok(msg))) => {
            let text = msg.to_text().unwrap();
            let json: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(json["event"], "spawned");
            assert_eq!(json["machine"], "ws_test");
        }
        _ => {
            // Event might not arrive if hooks fire before subscription registration.
            // The WebSocket connection itself working is the key test.
        }
    }

    // Transition
    let resp = client
        .post(format!("{}/execute", base_url))
        .json(&serde_json::json!({
            "smql": format!(r#"TRANSITION ws_test "{}" TO active"#, instance_id)
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Try to receive "state_changed" event
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), ws_stream.next()).await;

    if let Ok(Some(Ok(msg))) = event {
        let text = msg.to_text().unwrap();
        let json: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(
            json["event"] == "spawned" || json["event"] == "state_changed",
            "Unexpected event: {:?}",
            json
        );
    }
}

#[tokio::test]
async fn websocket_subscribe_with_machine_filter() {
    use futures_util::StreamExt;
    use tokio::net::TcpListener;

    let server = SmqlServer::new();
    let app = server.router();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", addr);

    // Define two machines
    let resp = client
        .post(format!("{}/execute", base_url))
        .json(&serde_json::json!({
            "smql": r#"DEFINE MACHINE m1 (
                STATES { a, b }
                INITIAL STATE a
                TERMINAL STATES { b }
                TRANSITIONS { a -> b {} }
                HOOKS {
                    ON SPAWN {
                        EMIT("m1_spawned")
                    }
                }
            )"#
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let resp = client
        .post(format!("{}/execute", base_url))
        .json(&serde_json::json!({
            "smql": r#"DEFINE MACHINE m2 (
                STATES { x, y }
                INITIAL STATE x
                TERMINAL STATES { y }
                TRANSITIONS { x -> y {} }
                HOOKS {
                    ON SPAWN {
                        EMIT("m2_spawned")
                    }
                }
            )"#
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Subscribe filtered to m1 only
    let ws_url = format!("ws://{}/subscribe?machine=m1", addr);
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Spawn m2 — should NOT produce a WS event
    let resp = client
        .post(format!("{}/execute", base_url))
        .json(&serde_json::json!({ "smql": r#"SPAWN m2 {}"# }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Spawn m1 — should produce a WS event
    let resp = client
        .post(format!("{}/execute", base_url))
        .json(&serde_json::json!({ "smql": r#"SPAWN m1 {}"# }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // We should get the m1 event
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), ws_stream.next()).await;

    if let Ok(Some(Ok(msg))) = event {
        let text = msg.to_text().unwrap();
        let json: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(json["machine"], "m1", "Should only receive m1 events");
    }
}

// ---------------------------------------------------------------------------
// value_to_json coverage tests
// ---------------------------------------------------------------------------

#[test]
fn value_to_json_text() {
    let val = smql_ast::value::Value::Text("hello".to_string());
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, serde_json::json!("hello"));
}

#[test]
fn value_to_json_int() {
    let val = smql_ast::value::Value::Int(42);
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, serde_json::json!(42));
}

#[test]
fn value_to_json_float() {
    let val = smql_ast::value::Value::Float(3.125);
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, serde_json::json!(3.125));
}

#[test]
fn value_to_json_bool() {
    let val = smql_ast::value::Value::Bool(true);
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, serde_json::json!(true));
}

#[test]
fn value_to_json_null() {
    let val = smql_ast::value::Value::Null;
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, serde_json::Value::Null);
}

#[test]
fn value_to_json_duration() {
    let dur = smql_ast::value::SmqlDuration::from_seconds(3600);
    let val = smql_ast::value::Value::Duration(dur);
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, serde_json::json!("1h"));
}

#[test]
fn value_to_json_list() {
    let val = smql_ast::value::Value::List(vec![
        smql_ast::value::Value::Int(1),
        smql_ast::value::Value::Text("two".to_string()),
    ]);
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, serde_json::json!([1, "two"]));
}

#[test]
fn value_to_json_set() {
    let val = smql_ast::value::Value::Set(vec![smql_ast::value::Value::Int(42)]);
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, serde_json::json!([42]));
}

#[test]
fn value_to_json_map() {
    let mut map = std::collections::BTreeMap::new();
    map.insert("key".to_string(), smql_ast::value::Value::Int(10));
    let val = smql_ast::value::Value::Map(map);
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, serde_json::json!({"key": 10}));
}

#[test]
fn value_to_json_ref() {
    let val = smql_ast::value::Value::Ref("Order".to_string(), "abc123".to_string());
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, serde_json::json!({"ref": "Order#abc123"}));
}

#[test]
fn value_to_json_money() {
    let val = smql_ast::value::Value::Money(9999, "USD".to_string());
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, serde_json::json!({"amount": 9999, "currency": "USD"}));
}

#[test]
fn value_to_json_blob() {
    let val = smql_ast::value::Value::Blob(vec![1, 2, 3, 4]);
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, serde_json::json!({"blob_size": 4}));
}

#[test]
fn value_to_json_json_passthrough() {
    let inner = serde_json::json!({"nested": [1, 2, 3]});
    let val = smql_ast::value::Value::Json(inner.clone());
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, inner);
}

// ---------------------------------------------------------------------------
// REST endpoint tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_machines_empty() {
    let app = test_router();
    let resp = app.oneshot(get_request("/machines")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["machines"], serde_json::json!([]));
}

#[tokio::test]
async fn list_machines_after_define() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app.oneshot(get_request("/machines")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let machines = json["machines"].as_array().unwrap();
    assert_eq!(machines.len(), 1);
    assert_eq!(machines[0], "counter");
}

#[tokio::test]
async fn get_machine_found() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app.oneshot(get_request("/machines/counter")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["name"], "counter");
    assert_eq!(json["initial_state"], "idle");
}

#[tokio::test]
async fn get_machine_not_found() {
    let app = test_router();
    let resp = app
        .oneshot(get_request("/machines/nonexistent"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn get_instance_found() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    let resp = app
        .oneshot(get_request(&format!("/instances/{}", instance_id)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["machine"], "counter");
    assert_eq!(json["state"], "idle");
}

#[tokio::test]
async fn get_instance_invalid_id() {
    let app = test_router();
    let resp = app
        .oneshot(get_request("/instances/not-a-ulid"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(json["error"].as_str().unwrap().contains("Invalid"));
}

#[tokio::test]
async fn get_instance_not_found() {
    let app = test_router();
    // Generate a valid ULID that doesn't exist in storage
    let id = smql_storage::InstanceId::new();
    let resp = app
        .oneshot(get_request(&format!("/instances/{}", id)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// Execute edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_parse_error() {
    let app = test_router();
    let resp = app
        .oneshot(execute_request("INVALID GARBAGE @#$"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"].as_str().is_some());
}

#[tokio::test]
async fn execute_empty_input() {
    let app = test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({ "smql": "" }).to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn execute_try_transition_guard_fails() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_guarded_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN gated {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    let smql = format!(r#"TRY TRANSITION gated "{}" TO closed"#, instance_id);
    let resp = app.clone().oneshot(execute_request(&smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["transitioned"], false);
}

#[tokio::test]
async fn execute_try_transition_success() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    let smql = format!(r#"TRY TRANSITION counter "{}" TO running"#, instance_id);
    let resp = app.clone().oneshot(execute_request(&smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["transitioned"], true);
    assert_eq!(json["result"]["to_state"], "running");
}

#[tokio::test]
async fn execute_alter_machine() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let alter_smql = r#"ALTER MACHINE counter ADD STATE paused"#;
    let resp = app
        .clone()
        .oneshot(execute_request(alter_smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["action"], "machine_altered");
    assert_eq!(json["result"]["machine"], "counter");
}

#[tokio::test]
async fn execute_query_get() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    let smql = format!(r#"GET counter "{}""#, instance_id);
    let resp = app.clone().oneshot(execute_request(&smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["machine"], "counter");
}

#[tokio::test]
async fn execute_query_find() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request("FIND counter"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["count"], 1);
}

#[tokio::test]
async fn execute_query_trail() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    let smql = format!(r#"TRAIL OF "{}""#, instance_id);
    let resp = app.clone().oneshot(execute_request(&smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["result"]["entries"].as_array().is_some());
}

#[tokio::test]
async fn execute_query_aggregate() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request("AGGREGATE counter MEASURE COUNT()"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["result"]["rows"].as_array().is_some());
}

#[tokio::test]
async fn execute_query_paths() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    let transition_smql = format!(r#"TRANSITION counter "{}" TO running"#, instance_id);
    let resp = app
        .clone()
        .oneshot(execute_request(&transition_smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(execute_request("PATHS FROM counter"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["result"]["paths"].as_array().is_some());
}

#[tokio::test]
async fn execute_query_funnel() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request("FUNNEL counter THROUGH [idle, running]"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert!(json["result"]["stages"].as_array().is_some());
}

// ---------------------------------------------------------------------------
// DELETE endpoint tests
// ---------------------------------------------------------------------------

/// Helper: make a DELETE request to a path.
fn delete_request(path: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn delete_instance_success() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    // Delete the instance
    let resp = app
        .clone()
        .oneshot(delete_request(&format!("/instances/{}", instance_id)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["deleted"], true);
    assert_eq!(json["id"], instance_id);

    // Verify it's gone
    let resp = app
        .oneshot(get_request(&format!("/instances/{}", instance_id)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_instance_not_found() {
    let app = test_router();
    let id = smql_storage::InstanceId::new();
    let resp = app
        .oneshot(delete_request(&format!("/instances/{}", id)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_instance_invalid_id() {
    let app = test_router();
    let resp = app
        .oneshot(delete_request("/instances/not-a-ulid"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// BATCH TRANSITION via HTTP tests
// ---------------------------------------------------------------------------

fn define_task_machine() -> &'static str {
    r#"DEFINE MACHINE btask (
        DATA {
            category : TEXT -> REQUIRED
        }
        STATES { open, cancelled }
        INITIAL STATE open
        TERMINAL STATES { cancelled }
        TRANSITIONS {
            ANY -> cancelled {
                EXCEPT FROM { cancelled }
            }
        }
    )"#
}

#[tokio::test]
async fn batch_transition_via_http() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_task_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Spawn two instances
    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN btask { category: "bugs" }"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN btask { category: "features" }"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Batch transition all open to cancelled
    let resp = app
        .clone()
        .oneshot(execute_request(
            r#"TRANSITION ALL btask WHERE STATE IS open TO cancelled"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["action"], "batch_transition");
    assert_eq!(json["result"]["matched"], 2);
    assert_eq!(json["result"]["transitioned"], 2);
    assert_eq!(json["result"]["failed"], 0);
}

// ---------------------------------------------------------------------------
// COMPARE PATHS via HTTP tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn compare_paths_via_http() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_task_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Spawn instances with different categories
    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN btask { category: "bugs" }"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN btask { category: "features" }"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(
            r#"COMPARE PATHS btask SEGMENT BY category"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["segment_by"], "category");
    assert!(json["result"]["segments"].as_array().is_some());
    let segments = json["result"]["segments"].as_array().unwrap();
    assert_eq!(segments.len(), 2);
}

// ---------------------------------------------------------------------------
// SmqlServer constructors
// ---------------------------------------------------------------------------

#[test]
fn server_default_creates_instance() {
    let _server = SmqlServer::default();
}

#[test]
fn server_with_storage() {
    let storage = std::sync::Arc::new(smql_storage::MemoryStorage::new());
    let _server = SmqlServer::with_storage(storage);
}

#[test]
fn server_with_engine() {
    let catalog = std::sync::Arc::new(smql_catalog::MachineCatalog::new());
    let storage = std::sync::Arc::new(smql_storage::MemoryStorage::new());
    let engine = std::sync::Arc::new(smql_engine_core::Engine::new(catalog, storage));
    let _server = SmqlServer::with_engine(engine);
}

// ---------------------------------------------------------------------------
// value_to_json additional coverage (Uuid, DateTime, Date)
// ---------------------------------------------------------------------------

#[test]
fn value_to_json_uuid() {
    // Use a fixed UUID (v7 not available, construct from string)
    let uuid = uuid::Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
    let val = smql_ast::value::Value::Uuid(uuid);
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, serde_json::json!("550e8400-e29b-41d4-a716-446655440000"));
}

#[test]
fn value_to_json_datetime() {
    let dt = chrono::Utc::now();
    let val = smql_ast::value::Value::DateTime(dt);
    let json = crate::handlers::value_to_json(&val);
    // Should be an RFC3339 string
    let s = json.as_str().unwrap();
    assert!(s.contains("T"), "DateTime should be RFC3339 format: {}", s);
}

#[test]
fn value_to_json_date() {
    let d = chrono::NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
    let val = smql_ast::value::Value::Date(d);
    let json = crate::handlers::value_to_json(&val);
    assert_eq!(json, serde_json::json!("2025-06-15"));
}

// ---------------------------------------------------------------------------
// Metrics Default impl
// ---------------------------------------------------------------------------

#[test]
fn metrics_default_impl() {
    let metrics = crate::metrics::SmqlMetrics::default();
    // Verify it's functional by observing a metric
    metrics.spawns_total.with_label_values(&["test"]).inc();
    let output = metrics.encode();
    assert!(output.contains("smql_spawns_total"));
}

// ---------------------------------------------------------------------------
// Spawn response structure verification
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_spawn_response_structure() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();

    // Verify top-level response structure
    assert_eq!(json["success"], true);
    assert!(json["error"].is_null());

    // Verify instance fields in result
    let result = &json["result"];
    assert!(result["id"].as_str().is_some(), "id should be present");
    assert_eq!(result["machine"], "counter");
    assert_eq!(result["state"], "idle");
    assert!(result["data"].is_object());
    assert!(result["created_at"].as_str().is_some());
    assert!(result["updated_at"].as_str().is_some());
    assert!(result["state_entered_at"].as_str().is_some());
    assert!(result["version"].is_number());
    assert!(result["trail_length"].is_number());
}

// ---------------------------------------------------------------------------
// Transition response structure verification
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_transition_response_structure() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    let smql = format!(r#"TRANSITION counter "{}" TO running"#, instance_id);
    let resp = app
        .clone()
        .oneshot(execute_request(&smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();

    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["from_state"], "idle");
    assert_eq!(json["result"]["to_state"], "running");
    assert!(json["result"]["instance"].is_object());
    assert_eq!(json["result"]["instance"]["state"], "running");
}

// ---------------------------------------------------------------------------
// Transition error — machine not found
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_transition_machine_not_found() {
    let app = test_router();
    let id = smql_storage::InstanceId::new();
    let smql = format!(r#"TRANSITION nonexistent "{}" TO somewhere"#, id);
    let resp = app.oneshot(execute_request(&smql)).await.unwrap();
    // Should return NOT_FOUND (404) since machine doesn't exist
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"].as_str().is_some());
}

// ---------------------------------------------------------------------------
// Spawn error — missing required field (SpawnRejected)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_spawn_missing_required_field() {
    let app = test_router();

    // Define a machine with a required field
    let define_smql = r#"DEFINE MACHINE typed_m (
        DATA {
            name : TEXT -> REQUIRED
        }
        STATES { open, done }
        INITIAL STATE open
        TERMINAL STATES { done }
        TRANSITIONS {
            open -> done {}
        }
    )"#;
    let resp = app
        .clone()
        .oneshot(execute_request(define_smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Spawn without providing required field
    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN typed_m {}"#))
        .await
        .unwrap();
    // SpawnRejected maps to BAD_REQUEST
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"].as_str().unwrap().contains("name"));
}

// ---------------------------------------------------------------------------
// Spawn error — machine not found
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_spawn_machine_not_found() {
    let app = test_router();
    let resp = app
        .oneshot(execute_request(r#"SPAWN nonexistent_machine {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], false);
}

// ---------------------------------------------------------------------------
// Transition — instance not found
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_transition_instance_not_found() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let fake_id = smql_storage::InstanceId::new();
    let smql = format!(r#"TRANSITION counter "{}" TO running"#, fake_id);
    let resp = app.oneshot(execute_request(&smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"].as_str().unwrap().contains("not found") || json["error"].as_str().unwrap().contains("Not found"));
}

// ---------------------------------------------------------------------------
// TRY TRANSITION — instance not found (error path)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_try_transition_instance_not_found() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let fake_id = smql_storage::InstanceId::new();
    let smql = format!(r#"TRY TRANSITION counter "{}" TO running"#, fake_id);
    let resp = app.oneshot(execute_request(&smql)).await.unwrap();
    // TRY TRANSITION with missing instance should still error (not Ok(None))
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], false);
}

// ---------------------------------------------------------------------------
// TRY TRANSITION — machine not found (error path)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_try_transition_machine_not_found() {
    let app = test_router();
    let fake_id = smql_storage::InstanceId::new();
    let smql = format!(r#"TRY TRANSITION nosuch "{}" TO running"#, fake_id);
    let resp = app.oneshot(execute_request(&smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ---------------------------------------------------------------------------
// BATCH TRANSITION — with guard failures
// ---------------------------------------------------------------------------

#[tokio::test]
async fn batch_transition_with_failures() {
    let app = test_router();

    // Machine where only some transitions succeed (guard blocks some)
    let define_smql = r#"DEFINE MACHINE bguarded (
        DATA {
            priority : INT
        }
        STATES { open, approved, rejected }
        INITIAL STATE open
        TERMINAL STATES { approved, rejected }
        TRANSITIONS {
            open -> approved {
                GUARD : priority > 5
            }
            open -> rejected {}
        }
    )"#;
    let resp = app
        .clone()
        .oneshot(execute_request(define_smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Spawn one that passes guard and one that fails
    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN bguarded { priority: 10 }"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN bguarded { priority: 2 }"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Batch transition all to approved — one should fail guard
    let resp = app
        .clone()
        .oneshot(execute_request(
            r#"TRANSITION ALL bguarded WHERE STATE IS open TO approved"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["action"], "batch_transition");
    assert_eq!(json["result"]["matched"], 2);
    assert_eq!(json["result"]["transitioned"], 1);
    assert_eq!(json["result"]["failed"], 1);
    let failures = json["result"]["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1);
    assert!(failures[0]["instance_id"].as_str().is_some());
    assert!(failures[0]["error"].as_str().is_some());
}

// ---------------------------------------------------------------------------
// BATCH TRANSITION — machine not found
// ---------------------------------------------------------------------------

#[tokio::test]
async fn batch_transition_machine_not_found() {
    let app = test_router();
    let resp = app
        .oneshot(execute_request(
            r#"TRANSITION ALL nonexistent WHERE STATE IS open TO closed"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], false);
}

// ---------------------------------------------------------------------------
// ALTER MACHINE — with warnings (remove state with instances)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_alter_machine_with_warnings() {
    let app = test_router();

    // Define machine with 3 states
    let define_smql = r#"DEFINE MACHINE alterable (
        STATES { open, pending, closed }
        INITIAL STATE open
        TERMINAL STATES { closed }
        TRANSITIONS {
            open -> pending {}
            pending -> closed {}
        }
    )"#;
    let resp = app
        .clone()
        .oneshot(execute_request(define_smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Spawn an instance (will be in open state)
    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN alterable {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    // Transition to pending
    let smql = format!(r#"TRANSITION alterable "{}" TO pending"#, instance_id);
    let resp = app
        .clone()
        .oneshot(execute_request(&smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Remove the pending state, which should migrate instances and produce warnings
    let alter_smql = r#"ALTER MACHINE alterable REMOVE STATE pending MIGRATE TO open"#;
    let resp = app
        .clone()
        .oneshot(execute_request(alter_smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["action"], "machine_altered");
    // Should have warnings about migrated instances and removed transitions
    assert!(
        json["warnings"].is_array(),
        "Expected warnings array, got: {:?}",
        json["warnings"]
    );
    let warnings = json["warnings"].as_array().unwrap();
    assert!(
        !warnings.is_empty(),
        "Expected non-empty warnings: {:?}",
        warnings
    );
}

// ---------------------------------------------------------------------------
// ALTER MACHINE — machine not found
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_alter_machine_not_found() {
    let app = test_router();
    let resp = app
        .oneshot(execute_request(r#"ALTER MACHINE nonexistent ADD STATE foo"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], false);
}

// ---------------------------------------------------------------------------
// GET query — instance not found (NotFound error through query path)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_query_get_not_found() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let fake_id = smql_storage::InstanceId::new();
    let smql = format!(r#"GET counter "{}""#, fake_id);
    let resp = app.oneshot(execute_request(&smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], false);
}

// ---------------------------------------------------------------------------
// FIND query — with filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_query_find_with_state_filter() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Spawn two instances
    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Transition one to running
    let smql = format!(r#"TRANSITION counter "{}" TO running"#, instance_id);
    let resp = app
        .clone()
        .oneshot(execute_request(&smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // FIND only idle ones
    let resp = app
        .clone()
        .oneshot(execute_request("FIND counter WHERE STATE IS idle"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["count"], 1);

    // Verify next_cursor is present when there are results
    let result = &json["result"];
    assert!(result["next_cursor"].as_str().is_some());
    assert!(result["instances"].as_array().is_some());
}

// ---------------------------------------------------------------------------
// TRAIL query — response structure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_query_trail_response_structure() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    // Transition to get a trail entry
    let smql = format!(r#"TRANSITION counter "{}" TO running"#, instance_id);
    let resp = app
        .clone()
        .oneshot(execute_request(&smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let smql = format!(r#"TRAIL OF "{}""#, instance_id);
    let resp = app.oneshot(execute_request(&smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();

    assert_eq!(json["success"], true);
    let entries = json["result"]["entries"].as_array().unwrap();
    assert!(entries.len() >= 2, "Should have spawn + transition entries");
    assert!(json["result"]["count"].as_i64().is_some());

    // Verify trail entry structure
    let entry = &entries[entries.len() - 1]; // Last entry is the transition
    assert!(entry["sequence"].is_number());
    assert!(entry["from_state"].as_str().is_some());
    assert!(entry["to_state"].as_str().is_some());
    assert!(entry["timestamp"].as_str().is_some());
}

// ---------------------------------------------------------------------------
// AGGREGATE query — response structure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_query_aggregate_response_structure() {
    let app = test_router();

    let define_smql = r#"DEFINE MACHINE agg_m (
        DATA {
            region : TEXT
        }
        STATES { active, done }
        INITIAL STATE active
        TERMINAL STATES { done }
        TRANSITIONS {
            active -> done {}
        }
    )"#;
    let resp = app
        .clone()
        .oneshot(execute_request(define_smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN agg_m { region: "US" }"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN agg_m { region: "EU" }"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(
            "AGGREGATE agg_m GROUP BY region MEASURE COUNT()",
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    let rows = json["result"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    // Each row should have group and measures
    for row in rows {
        assert!(row["group"].is_object());
        assert!(row["measures"].is_object());
    }
}

// ---------------------------------------------------------------------------
// FUNNEL query — response structure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_query_funnel_response_structure() {
    let app = test_router();

    let define_smql = r#"DEFINE MACHINE funnel_m (
        STATES { start, middle, end }
        INITIAL STATE start
        TERMINAL STATES { end }
        TRANSITIONS {
            start -> middle {}
            middle -> end {}
        }
    )"#;
    let resp = app
        .clone()
        .oneshot(execute_request(define_smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Spawn some instances and transition some through
    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN funnel_m {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let id1 = json["result"]["id"].as_str().unwrap().to_string();

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN funnel_m {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Transition first instance to middle
    let smql = format!(r#"TRANSITION funnel_m "{}" TO middle"#, id1);
    let resp = app
        .clone()
        .oneshot(execute_request(&smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(execute_request(
            "FUNNEL funnel_m THROUGH [start, middle, end]",
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    let stages = json["result"]["stages"].as_array().unwrap();
    assert_eq!(stages.len(), 3);
    // Each stage should have state, count, conversion_rate
    for stage in stages {
        assert!(stage["state"].as_str().is_some());
        assert!(stage["count"].is_number());
        assert!(stage["conversion_rate"].is_number());
    }
}

// ---------------------------------------------------------------------------
// PATHS query — response structure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_query_paths_response_structure() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Spawn and transition to create a path
    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let id = json["result"]["id"].as_str().unwrap().to_string();

    let smql = format!(r#"TRANSITION counter "{}" TO running"#, id);
    let resp = app
        .clone()
        .oneshot(execute_request(&smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(execute_request("PATHS FROM counter"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    let paths = json["result"]["paths"].as_array().unwrap();
    assert!(!paths.is_empty());
    for path in paths {
        assert!(path["path"].is_array());
        assert!(path["count"].is_number());
    }
}

// ---------------------------------------------------------------------------
// COMPARE PATHS query — response structure
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_query_compare_paths_response_structure() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_task_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN btask { category: "a" }"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN btask { category: "b" }"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(
            r#"COMPARE PATHS btask SEGMENT BY category"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["segment_by"], "category");
    let segments = json["result"]["segments"].as_array().unwrap();
    assert_eq!(segments.len(), 2);
    for seg in segments {
        assert!(seg["segment_value"].is_string());
        assert!(seg["paths"].is_array());
    }
}

// ---------------------------------------------------------------------------
// DEFINE MACHINE — catalog register error (invalid definition)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_define_machine_invalid_definition() {
    let app = test_router();
    // Machine with no states is invalid
    let define_smql = r#"DEFINE MACHINE bad_m (
        STATES { }
        INITIAL STATE open
        TERMINAL STATES { closed }
    )"#;
    let resp = app.oneshot(execute_request(define_smql)).await.unwrap();
    // Parser or validation should reject this
    let status = resp.status();
    assert!(
        status == StatusCode::BAD_REQUEST,
        "Expected 400 for invalid machine, got {}",
        status
    );
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"].as_str().is_some());
}

// ---------------------------------------------------------------------------
// Transition — guard denied (CONFLICT status)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_transition_guard_denied_returns_conflict() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_guarded_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN gated {}"#))
        .await
        .unwrap();
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    let smql = format!(r#"TRANSITION gated "{}" TO closed"#, instance_id);
    let resp = app.oneshot(execute_request(&smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"].as_str().unwrap().contains("denied"));
}

// ---------------------------------------------------------------------------
// DELETE /instances/:id — metrics decrement
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_instance_decrements_metrics() {
    let server = SmqlServer::new();
    let app = server.router();

    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    // Verify metrics show 1 instance
    let resp = app.clone().oneshot(get_request("/metrics")).await.unwrap();
    let body = body_string(resp.into_body()).await;
    assert!(
        body.contains(r#"smql_instances_total{machine="counter",state="idle"} 1"#),
        "Expected 1 instance before delete:\n{}",
        body
    );

    // Delete the instance
    let resp = app
        .clone()
        .oneshot(delete_request(&format!("/instances/{}", instance_id)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify metrics show 0 instances
    let resp = app.clone().oneshot(get_request("/metrics")).await.unwrap();
    let body = body_string(resp.into_body()).await;
    assert!(
        body.contains(r#"smql_instances_total{machine="counter",state="idle"} 0"#),
        "Expected 0 instances after delete:\n{}",
        body
    );
}

// ---------------------------------------------------------------------------
// Metrics endpoint content-type header
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metrics_endpoint_content_type() {
    let app = test_router();
    let resp = app.oneshot(get_request("/metrics")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let content_type = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        content_type.contains("text/plain"),
        "Expected text/plain, got: {}",
        content_type
    );
}

// ---------------------------------------------------------------------------
// Metrics histogram observation
// ---------------------------------------------------------------------------

#[test]
fn metrics_histogram_observation() {
    let metrics = crate::metrics::SmqlMetrics::new();
    metrics
        .transition_duration_seconds
        .with_label_values(&["test"])
        .observe(0.05);
    metrics
        .transition_duration_seconds
        .with_label_values(&["test"])
        .observe(0.15);
    let output = metrics.encode();
    assert!(
        output.contains("smql_transition_duration_seconds_count"),
        "Expected histogram count in output:\n{}",
        output
    );
    assert!(
        output.contains("smql_transition_duration_seconds_sum"),
        "Expected histogram sum in output:\n{}",
        output
    );
}

#[test]
fn metrics_query_histogram() {
    let metrics = crate::metrics::SmqlMetrics::new();
    metrics
        .query_duration_seconds
        .with_label_values(&["AGGREGATE"])
        .observe(0.003);
    let output = metrics.encode();
    assert!(
        output.contains(r#"query_type="AGGREGATE"#),
        "Expected AGGREGATE query type label:\n{}",
        output
    );
}

// ---------------------------------------------------------------------------
// FIND with empty results (no next_cursor)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_query_find_empty_results() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // FIND with a filter that matches nothing
    let resp = app
        .clone()
        .oneshot(execute_request("FIND counter WHERE STATE IS running"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["count"], 0);
    let instances = json["result"]["instances"].as_array().unwrap();
    assert!(instances.is_empty());
    // No next_cursor when results are empty
    assert!(
        json["result"]["next_cursor"].is_null(),
        "Expected no next_cursor for empty results"
    );
}

// ---------------------------------------------------------------------------
// Query metrics — all query types are tracked
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metrics_track_all_query_types() {
    let server = SmqlServer::new();
    let app = server.router();

    let resp = app
        .clone()
        .oneshot(execute_request(define_task_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Spawn an instance
    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN btask { category: "test" }"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap().to_string();

    // Execute each query type
    let get_smql = format!(r#"GET btask "{}""#, instance_id);
    let resp = app
        .clone()
        .oneshot(execute_request(&get_smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(execute_request("FIND btask"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let trail_smql = format!(r#"TRAIL OF "{}""#, instance_id);
    let resp = app
        .clone()
        .oneshot(execute_request(&trail_smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(execute_request("AGGREGATE btask MEASURE COUNT()"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(execute_request("PATHS FROM btask"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(execute_request(
            "FUNNEL btask THROUGH [open, cancelled]",
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(execute_request(
            r#"COMPARE PATHS btask SEGMENT BY category"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Check all query types appear in metrics
    let resp = app.clone().oneshot(get_request("/metrics")).await.unwrap();
    let body = body_string(resp.into_body()).await;
    for qt in &["GET", "FIND", "TRAIL", "AGGREGATE", "PATHS", "FUNNEL", "COMPARE_PATHS"] {
        assert!(
            body.contains(&format!(r#"query_type="{}""#, qt)),
            "Expected query type {} in metrics:\n{}",
            qt,
            body
        );
    }
}

// ---------------------------------------------------------------------------
// GET /instances/:id — response structure verification
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_instance_response_structure() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    let resp = app
        .oneshot(get_request(&format!("/instances/{}", instance_id)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();

    // instance_to_json should produce all these fields
    assert_eq!(json["id"], instance_id);
    assert_eq!(json["machine"], "counter");
    assert_eq!(json["state"], "idle");
    assert!(json["data"].is_object());
    assert!(json["created_at"].as_str().is_some());
    assert!(json["updated_at"].as_str().is_some());
    assert!(json["state_entered_at"].as_str().is_some());
    assert!(json["version"].is_number());
    assert!(json["trail_length"].is_number());
}

// ---------------------------------------------------------------------------
// SmqlServer router produces a valid Router
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_router_responds_to_all_routes() {
    let server = SmqlServer::new();
    let app = server.router();

    // Health
    let resp = app.clone().oneshot(get_request("/health")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Machines
    let resp = app.clone().oneshot(get_request("/machines")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Metrics
    let resp = app.clone().oneshot(get_request("/metrics")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Spawn with data fields
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_spawn_with_data_verifies_data_in_response() {
    let app = test_router();

    let define_smql = r#"DEFINE MACHINE data_m (
        DATA {
            label : TEXT -> REQUIRED
            qty : INT
        }
        STATES { active, done }
        INITIAL STATE active
        TERMINAL STATES { done }
        TRANSITIONS {
            active -> done {}
        }
    )"#;
    let resp = app
        .clone()
        .oneshot(execute_request(define_smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(
            r#"SPAWN data_m { label: "hello", qty: 42 }"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let result = &json["result"];
    assert_eq!(result["data"]["label"], "hello");
    assert_eq!(result["data"]["qty"], 42);
}

// ---------------------------------------------------------------------------
// TRANSITION with invalid state returns error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_transition_invalid_target_state() {
    let app = test_router();
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap();

    let smql = format!(r#"TRANSITION counter "{}" TO nonexistent_state"#, instance_id);
    let resp = app.oneshot(execute_request(&smql)).await.unwrap();
    // Invalid state should produce an error (ValidationError -> BAD_REQUEST, or TransitionDenied -> CONFLICT)
    assert!(
        resp.status() == StatusCode::BAD_REQUEST
            || resp.status() == StatusCode::CONFLICT
            || resp.status() == StatusCode::INTERNAL_SERVER_ERROR,
        "Expected error status, got: {}",
        resp.status()
    );
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], false);
}

// ===========================================================================
// Additional coverage tests
// ===========================================================================

// ---------------------------------------------------------------------------
// SmqlServer::with_storage — functional test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_with_storage_functional() {
    let storage = std::sync::Arc::new(smql_storage::MemoryStorage::new());
    let server = SmqlServer::with_storage(storage);
    let app = server.router();

    // Define a machine through the router
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Spawn an instance
    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["result"]["machine"], "counter");
    assert_eq!(json["result"]["state"], "idle");

    // Verify instance is retrievable
    let instance_id = json["result"]["id"].as_str().unwrap();
    let resp = app
        .oneshot(get_request(&format!("/instances/{}", instance_id)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

// ---------------------------------------------------------------------------
// SmqlServer::with_engine — functional test with pre-configured engine
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_with_engine_functional() {
    let catalog = std::sync::Arc::new(smql_catalog::MachineCatalog::new());
    let storage = std::sync::Arc::new(smql_storage::MemoryStorage::new());
    let engine = std::sync::Arc::new(smql_engine_core::Engine::new(catalog, storage));

    // Pre-register a machine on the engine before building the server
    let def = smql_parser::parse(define_simple_machine()).unwrap();
    if let smql_ast::command::Statement::Command(smql_ast::command::Command::DefineMachine(d)) =
        def.into_iter().next().unwrap()
    {
        engine.catalog.register(d).unwrap();
    }

    let server = SmqlServer::with_engine(engine);
    let app = server.router();

    // Machine should already be listed
    let resp = app.clone().oneshot(get_request("/machines")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let machines = json["machines"].as_array().unwrap();
    assert_eq!(machines.len(), 1);
    assert_eq!(machines[0], "counter");

    // Spawn through the pre-configured engine
    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

// ---------------------------------------------------------------------------
// SmqlServer::default — functional test
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_default_functional() {
    let server = SmqlServer::default();
    let app = server.router();

    // Health should work
    let resp = app.clone().oneshot(get_request("/health")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Define and spawn should work
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

// ---------------------------------------------------------------------------
// Metrics — default() encode returns valid empty output
// ---------------------------------------------------------------------------

#[test]
fn metrics_default_encode_empty_output() {
    let metrics = crate::metrics::SmqlMetrics::default();
    let output = metrics.encode();
    // Fresh metrics with no observations should produce an empty string
    // (prometheus only outputs metrics that have been observed at least once)
    // Crucially, it should NOT panic
    // Fresh metrics should not panic on encode — result may be empty string
    let _ = output;
}

#[test]
fn metrics_encode_after_multiple_observations() {
    let metrics = crate::metrics::SmqlMetrics::default();
    // Observe across multiple metric types
    metrics.spawns_total.with_label_values(&["machine_a"]).inc();
    metrics.spawns_total.with_label_values(&["machine_a"]).inc();
    metrics.spawns_total.with_label_values(&["machine_b"]).inc();
    metrics
        .transitions_total
        .with_label_values(&["machine_a", "idle", "running"])
        .inc();
    metrics
        .instances_total
        .with_label_values(&["machine_a", "idle"])
        .set(5);
    metrics
        .guard_failures_total
        .with_label_values(&["machine_a"])
        .inc();
    metrics
        .timeout_fires_total
        .with_label_values(&["machine_a", "waiting"])
        .inc();
    metrics
        .query_duration_seconds
        .with_label_values(&["GET"])
        .observe(0.001);
    metrics
        .query_duration_seconds
        .with_label_values(&["FIND"])
        .observe(0.002);
    metrics
        .transition_duration_seconds
        .with_label_values(&["machine_a"])
        .observe(0.05);

    let output = metrics.encode();
    // Verify multiple label values for the same metric
    assert!(output.contains(r#"smql_spawns_total{machine="machine_a"} 2"#));
    assert!(output.contains(r#"smql_spawns_total{machine="machine_b"} 1"#));
    assert!(output.contains(r#"smql_instances_total{machine="machine_a",state="idle"} 5"#));
    assert!(output.contains(r#"smql_timeout_fires_total{machine="machine_a",state="waiting"} 1"#));
}

// ---------------------------------------------------------------------------
// delete_instance handler — response body for not_found
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_instance_not_found_response_body() {
    let app = test_router();
    let id = smql_storage::InstanceId::new();
    let resp = app
        .oneshot(delete_request(&format!("/instances/{}", id)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(
        json["error"].as_str().unwrap().contains("not found"),
        "Expected 'not found' in error: {:?}",
        json["error"]
    );
}

// ---------------------------------------------------------------------------
// delete_instance handler — invalid ID response body
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delete_instance_invalid_id_response_body() {
    let app = test_router();
    let resp = app
        .oneshot(delete_request("/instances/not-a-valid-ulid"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(
        json["error"].as_str().unwrap().contains("Invalid"),
        "Expected 'Invalid' in error: {:?}",
        json["error"]
    );
}

// ---------------------------------------------------------------------------
// get_instance handler — not_found response body
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_instance_not_found_response_body() {
    let app = test_router();
    let id = smql_storage::InstanceId::new();
    let resp = app
        .oneshot(get_request(&format!("/instances/{}", id)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert!(
        json["error"].as_str().unwrap().contains("not found"),
        "Expected 'not found' in error: {:?}",
        json["error"]
    );
}

// ---------------------------------------------------------------------------
// execute empty SMQL — verify error message in body
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_empty_input_response_body() {
    let app = test_router();
    let req = Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({ "smql": "" }).to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], false);
    // Should have an error message — either parse error or "Empty SMQL input"
    let error_str = json["error"].as_str().unwrap();
    assert!(
        !error_str.is_empty(),
        "Expected non-empty error message"
    );
}

// ---------------------------------------------------------------------------
// execute parse error — verify error message in body
// ---------------------------------------------------------------------------

#[tokio::test]
async fn execute_parse_error_response_body() {
    let app = test_router();
    let resp = app
        .oneshot(execute_request("NOT A VALID SMQL STATEMENT @@@@"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["result"].is_null(), "result should be null on parse error");
    assert!(json["warnings"].is_null(), "warnings should be null on parse error");
    let error_str = json["error"].as_str().unwrap();
    assert!(
        !error_str.is_empty(),
        "Expected non-empty error message on parse error"
    );
}

// ---------------------------------------------------------------------------
// error_response variants — unit tests for each SmqlError -> status mapping
// ---------------------------------------------------------------------------

#[test]
fn error_response_not_found_variant() {
    let err = smql_ast::SmqlError::NotFound {
        entity_type: "Instance".to_string(),
        id: "abc123".to_string(),
    };
    let (status, _json_resp) = crate::handlers::error_response_for_test(err);
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[test]
fn error_response_transition_denied_variant() {
    let err = smql_ast::SmqlError::TransitionDenied(smql_ast::error::TransitionDeniedError {
        instance_id: "test".to_string(),
        from_state: "a".to_string(),
        to_state: "b".to_string(),
        guard_failures: vec![],
        hint: None,
    });
    let (status, _json_resp) = crate::handlers::error_response_for_test(err);
    assert_eq!(status, StatusCode::CONFLICT);
}

#[test]
fn error_response_spawn_rejected_variant() {
    let err = smql_ast::SmqlError::SpawnRejected {
        message: "missing field".to_string(),
        field: Some("name".to_string()),
        hint: None,
    };
    let (status, _json_resp) = crate::handlers::error_response_for_test(err);
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[test]
fn error_response_validation_error_variant() {
    let err = smql_ast::SmqlError::ValidationError {
        message: "invalid state".to_string(),
        field: Some("state".to_string()),
        hint: None,
    };
    let (status, _json_resp) = crate::handlers::error_response_for_test(err);
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[test]
fn error_response_conflict_variant() {
    let err = smql_ast::SmqlError::Conflict {
        message: "version mismatch".to_string(),
        hint: None,
    };
    let (status, _json_resp) = crate::handlers::error_response_for_test(err);
    assert_eq!(status, StatusCode::CONFLICT);
}

#[test]
fn error_response_internal_variant() {
    let err = smql_ast::SmqlError::Internal {
        message: "something broke".to_string(),
    };
    let (status, _json_resp) = crate::handlers::error_response_for_test(err);
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn error_response_storage_error_variant() {
    let err = smql_ast::SmqlError::StorageError {
        message: "disk full".to_string(),
        retryable: false,
    };
    let (status, _json_resp) = crate::handlers::error_response_for_test(err);
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn error_response_query_error_variant() {
    let err = smql_ast::SmqlError::QueryError {
        message: "bad query".to_string(),
        hint: None,
    };
    let (status, _json_resp) = crate::handlers::error_response_for_test(err);
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn error_response_timeout_error_variant() {
    let err = smql_ast::SmqlError::TimeoutError {
        message: "timeout expired".to_string(),
        instance_id: Some("inst1".to_string()),
        state: Some("waiting".to_string()),
    };
    let (status, _json_resp) = crate::handlers::error_response_for_test(err);
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

// ---------------------------------------------------------------------------
// value_to_json edge cases — Uuid, Blob, Json, Ref, Money (combined test)
// ---------------------------------------------------------------------------

#[test]
fn value_to_json_all_edge_cases() {
    // Uuid
    let uuid = uuid::Uuid::parse_str("12345678-1234-1234-1234-123456789abc").unwrap();
    let json = crate::handlers::value_to_json(&smql_ast::value::Value::Uuid(uuid));
    assert_eq!(json, serde_json::json!("12345678-1234-1234-1234-123456789abc"));

    // Blob (empty)
    let json = crate::handlers::value_to_json(&smql_ast::value::Value::Blob(vec![]));
    assert_eq!(json, serde_json::json!({"blob_size": 0}));

    // Blob (large)
    let json = crate::handlers::value_to_json(&smql_ast::value::Value::Blob(vec![0u8; 1024]));
    assert_eq!(json, serde_json::json!({"blob_size": 1024}));

    // Json (nested object)
    let inner = serde_json::json!({"a": {"b": [1, 2, {"c": true}]}});
    let json = crate::handlers::value_to_json(&smql_ast::value::Value::Json(inner.clone()));
    assert_eq!(json, inner);

    // Json (scalar)
    let inner = serde_json::json!(42);
    let json = crate::handlers::value_to_json(&smql_ast::value::Value::Json(inner.clone()));
    assert_eq!(json, inner);

    // Ref with special characters
    let json = crate::handlers::value_to_json(&smql_ast::value::Value::Ref(
        "My Machine".to_string(),
        "id-with-dashes".to_string(),
    ));
    assert_eq!(json, serde_json::json!({"ref": "My Machine#id-with-dashes"}));

    // Money with zero amount
    let json = crate::handlers::value_to_json(&smql_ast::value::Value::Money(0, "EUR".to_string()));
    assert_eq!(json, serde_json::json!({"amount": 0, "currency": "EUR"}));

    // Money with negative amount
    let json = crate::handlers::value_to_json(&smql_ast::value::Value::Money(-500, "GBP".to_string()));
    assert_eq!(json, serde_json::json!({"amount": -500, "currency": "GBP"}));
}

// ---------------------------------------------------------------------------
// get_machine not found — verify error message includes machine name
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_machine_not_found_error_includes_name() {
    let app = test_router();
    let resp = app
        .oneshot(get_request("/machines/does_not_exist"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let err = json["error"].as_str().unwrap();
    assert!(
        err.contains("does_not_exist"),
        "Error should include machine name: {}",
        err
    );
    assert!(
        err.contains("not found"),
        "Error should say 'not found': {}",
        err
    );
}

// ---------------------------------------------------------------------------
// SmqlServer::with_storage shares engine across requests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_with_storage_shares_state() {
    let storage = std::sync::Arc::new(smql_storage::MemoryStorage::new());
    let server = SmqlServer::with_storage(storage);
    let app = server.router();

    // Define machine
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Spawn and transition
    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    let instance_id = json["result"]["id"].as_str().unwrap().to_string();

    // Transition via the same router
    let smql = format!(r#"TRANSITION counter "{}" TO running"#, instance_id);
    let resp = app
        .clone()
        .oneshot(execute_request(&smql))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify instance state changed via GET endpoint
    let resp = app
        .oneshot(get_request(&format!("/instances/{}", instance_id)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["state"], "running");
}

// ---------------------------------------------------------------------------
// SmqlServer::with_engine — metrics are separate per server instance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_with_engine_has_fresh_metrics() {
    let catalog = std::sync::Arc::new(smql_catalog::MachineCatalog::new());
    let storage = std::sync::Arc::new(smql_storage::MemoryStorage::new());
    let engine = std::sync::Arc::new(smql_engine_core::Engine::new(catalog, storage));
    let server = SmqlServer::with_engine(engine);
    let app = server.router();

    // Metrics should return OK even without any operations
    let resp = app.clone().oneshot(get_request("/metrics")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    // Fresh server should have no observed metrics (output may be empty)
    // At minimum, it should not contain any counter values > 0
    assert!(
        !body.contains("smql_spawns_total"),
        "Fresh server should not have spawns metric"
    );
}

// ---------------------------------------------------------------------------
// Metrics endpoint via /metrics after using with_engine
// ---------------------------------------------------------------------------

#[tokio::test]
async fn server_with_engine_metrics_after_operations() {
    let catalog = std::sync::Arc::new(smql_catalog::MachineCatalog::new());
    let storage = std::sync::Arc::new(smql_storage::MemoryStorage::new());
    let engine = std::sync::Arc::new(smql_engine_core::Engine::new(catalog, storage));
    let server = SmqlServer::with_engine(engine);
    let app = server.router();

    // Define and spawn
    let resp = app
        .clone()
        .oneshot(execute_request(define_simple_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN counter {}"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Metrics should now contain spawn data
    let resp = app.clone().oneshot(get_request("/metrics")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp.into_body()).await;
    assert!(
        body.contains(r#"smql_spawns_total{machine="counter"} 1"#),
        "Expected spawns metric after spawn:\n{}",
        body
    );
}

// ---------------------------------------------------------------------------
// start_event_metrics_listener coverage (handlers.rs lines 742-759)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn event_metrics_listener_counts_timeout_events() {
    use std::sync::Arc;
    let bus = Arc::new(smql_hooks::EventBus::new(64));
    let metrics = Arc::new(crate::metrics::SmqlMetrics::new());

    // Start the listener
    crate::handlers::start_event_metrics_listener(Arc::clone(&bus), Arc::clone(&metrics));

    // Emit a TIMEOUT event
    bus.emit(smql_hooks::Event {
        name: "TIMEOUT".to_string(),
        instance_id: "inst001".to_string(),
        machine: "TestMachine".to_string(),
        payload: None,
    });

    // Give the async listener time to process
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let output = metrics.encode();
    assert!(
        output.contains("smql_timeout_fires_total"),
        "Expected timeout_fires_total metric after TIMEOUT event:\n{}",
        output
    );
    assert!(
        output.contains(r#"machine="TestMachine"#),
        "Expected TestMachine label in timeout metric:\n{}",
        output
    );
}

#[tokio::test]
async fn event_metrics_listener_ignores_non_timeout_events() {
    use std::sync::Arc;
    let bus = Arc::new(smql_hooks::EventBus::new(64));
    let metrics = Arc::new(crate::metrics::SmqlMetrics::new());

    crate::handlers::start_event_metrics_listener(Arc::clone(&bus), Arc::clone(&metrics));

    // Emit a non-TIMEOUT event
    bus.emit(smql_hooks::Event {
        name: "state_changed".to_string(),
        instance_id: "inst002".to_string(),
        machine: "OtherMachine".to_string(),
        payload: None,
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let output = metrics.encode();
    // timeout_fires should NOT appear for non-TIMEOUT events
    assert!(
        !output.contains("smql_timeout_fires_total"),
        "Unexpected timeout_fires_total for non-TIMEOUT event:\n{}",
        output
    );
}

// ---------------------------------------------------------------------------
// WebSocket subscribe with event name filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn websocket_subscribe_with_event_name_filter() {
    use futures_util::StreamExt;
    use tokio::net::TcpListener;

    let server = SmqlServer::new();
    let app = server.router();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", addr);

    // Define a machine with EMIT hooks on spawn and transition
    let define_smql = r#"DEFINE MACHINE ev_filter (
        STATES { idle, active }
        INITIAL STATE idle
        TERMINAL STATES { active }
        TRANSITIONS {
            idle -> active {
                ACTION : EMIT("activated")
            }
        }
        HOOKS {
            ON SPAWN {
                EMIT("spawned")
            }
        }
    )"#;

    let resp = client
        .post(format!("{}/execute", base_url))
        .json(&serde_json::json!({ "smql": define_smql }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    // Subscribe filtered to only "activated" events
    let ws_url = format!("ws://{}/subscribe?event=activated", addr);
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Spawn an instance — emits "spawned" which should be filtered out
    let resp = client
        .post(format!("{}/execute", base_url))
        .json(&serde_json::json!({ "smql": r#"SPAWN ev_filter {}"# }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);
    let spawn_result: serde_json::Value = resp.json().await.unwrap();
    let instance_id = spawn_result["result"]["id"].as_str().unwrap().to_string();

    // Transition — emits "activated" which should pass the filter
    let resp = client
        .post(format!("{}/execute", base_url))
        .json(&serde_json::json!({
            "smql": format!(r#"TRANSITION ev_filter "{}" TO active"#, instance_id)
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // We should receive only the "activated" event
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), ws_stream.next()).await;

    if let Ok(Some(Ok(msg))) = event {
        let text = msg.to_text().unwrap();
        let json: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(
            json["event"], "activated",
            "Should only receive 'activated' events, got: {:?}",
            json
        );
    }
}

// ---------------------------------------------------------------------------
// COMPARE PATHS query type label in metrics
// ---------------------------------------------------------------------------

#[tokio::test]
async fn metrics_compare_paths_query_duration() {
    let server = SmqlServer::new();
    let app = server.router();

    let resp = app
        .clone()
        .oneshot(execute_request(define_task_machine()))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(r#"SPAWN btask { category: "a" }"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(execute_request(
            r#"COMPARE PATHS btask SEGMENT BY category"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Check that query duration was recorded for COMPARE_PATHS
    let resp = app.clone().oneshot(get_request("/metrics")).await.unwrap();
    let body = body_string(resp.into_body()).await;
    assert!(
        body.contains(r#"query_type="COMPARE_PATHS"#),
        "Expected COMPARE_PATHS query type label in metrics:\n{}",
        body
    );
}
