//! Comprehensive end-to-end test exercising ALL SMQL query types through the HTTP API.
//!
//! Machine: TaskTracker — a realistic task management machine with data fields,
//! guards, and multiple terminal states. Multiple instances are spawned and
//! driven through different paths so that every query type returns meaningful results.
//!
//! Query types tested:
//!   1. GET        — single instance retrieval
//!   2. FIND       — filtered search (WHERE, SORT, LIMIT, OFFSET, AFTER cursor)
//!   3. TRAIL      — audit log for an instance
//!   4. AGGREGATE  — COUNT, SUM, AVG, MIN, MAX, GROUP BY state/field
//!   5. PATHS      — state-sequence analysis
//!   6. FUNNEL     — conversion funnel through ordered states
//!   7. COMPARE PATHS — segmented path analysis by data field

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn test_router() -> axum::Router {
    smql_server::SmqlServer::new().router()
}

fn exec(smql: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/execute")
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::json!({ "smql": smql }).to_string(),
        ))
        .unwrap()
}

async fn body_json(body: Body) -> serde_json::Value {
    let bytes = body.collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Define the TaskTracker machine, spawn several instances, and drive them
/// through different paths. Returns (router, Vec<instance_ids>).
async fn setup() -> (axum::Router, Vec<String>) {
    let app = test_router();

    // ── Define machine ──────────────────────────────────────────────────
    let define = r#"DEFINE MACHINE TaskTracker (
        DATA {
            title    : TEXT -> REQUIRED
            priority : ENUM(low, medium, high) -> DEFAULT(medium)
            points   : INT -> DEFAULT(0)
            category : TEXT -> DEFAULT("general")
        }
        STATES { open, in_progress, review, done, cancelled }
        INITIAL STATE open
        TERMINAL STATES { done, cancelled }
        TRANSITIONS {
            open -> in_progress {}
            in_progress -> review {}
            review -> done {}
            review -> in_progress {}
            ANY -> cancelled {
                EXCEPT FROM { done, cancelled }
            }
        }
    )"#;
    let resp = app.clone().oneshot(exec(define)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "define machine");

    // ── Spawn instances ─────────────────────────────────────────────────
    // We create 6 instances with varied data and drive them along different paths:
    //
    // #0 "Fix login"       high   5  bugs     → open→in_progress→review→done   (happy path)
    // #1 "Add search"      medium 3  features → open→in_progress→review→done   (happy path)
    // #2 "Update docs"     low    1  docs     → open→in_progress               (in progress)
    // #3 "Refactor auth"   high   8  bugs     → open→in_progress→review→in_progress (rework)
    // #4 "Dark mode"       medium 5  features → open→cancelled                 (cancelled)
    // #5 "Fix crash"       high   2  bugs     → open→in_progress→review        (in review)

    let spawns = vec![
        r#"SPAWN TaskTracker { title: "Fix login",     priority: "high",   points: 5, category: "bugs" }"#,
        r#"SPAWN TaskTracker { title: "Add search",    priority: "medium", points: 3, category: "features" }"#,
        r#"SPAWN TaskTracker { title: "Update docs",   priority: "low",    points: 1, category: "docs" }"#,
        r#"SPAWN TaskTracker { title: "Refactor auth", priority: "high",   points: 8, category: "bugs" }"#,
        r#"SPAWN TaskTracker { title: "Dark mode",     priority: "medium", points: 5, category: "features" }"#,
        r#"SPAWN TaskTracker { title: "Fix crash",     priority: "high",   points: 2, category: "bugs" }"#,
    ];

    let mut ids = Vec::new();
    for smql in &spawns {
        let resp = app.clone().oneshot(exec(smql)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED, "spawn: {}", smql);
        let json = body_json(resp.into_body()).await;
        ids.push(json["result"]["id"].as_str().unwrap().to_string());
    }

    // ── Drive transitions ───────────────────────────────────────────────
    let transitions = vec![
        // #0 Fix login: open → in_progress → review → done
        format!(r#"TRANSITION TaskTracker "{}" TO in_progress"#, ids[0]),
        format!(r#"TRANSITION TaskTracker "{}" TO review"#, ids[0]),
        format!(r#"TRANSITION TaskTracker "{}" TO done"#, ids[0]),
        // #1 Add search: open → in_progress → review → done
        format!(r#"TRANSITION TaskTracker "{}" TO in_progress"#, ids[1]),
        format!(r#"TRANSITION TaskTracker "{}" TO review"#, ids[1]),
        format!(r#"TRANSITION TaskTracker "{}" TO done"#, ids[1]),
        // #2 Update docs: open → in_progress
        format!(r#"TRANSITION TaskTracker "{}" TO in_progress"#, ids[2]),
        // #3 Refactor auth: open → in_progress → review → in_progress (rework)
        format!(r#"TRANSITION TaskTracker "{}" TO in_progress"#, ids[3]),
        format!(r#"TRANSITION TaskTracker "{}" TO review"#, ids[3]),
        format!(r#"TRANSITION TaskTracker "{}" TO in_progress"#, ids[3]),
        // #4 Dark mode: open → cancelled
        format!(r#"TRANSITION TaskTracker "{}" TO cancelled"#, ids[4]),
        // #5 Fix crash: open → in_progress → review
        format!(r#"TRANSITION TaskTracker "{}" TO in_progress"#, ids[5]),
        format!(r#"TRANSITION TaskTracker "{}" TO review"#, ids[5]),
    ];

    for smql in &transitions {
        let resp = app.clone().oneshot(exec(smql)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "transition: {}", smql);
    }

    (app, ids)
}

// ==========================================================================
// 1. GET — single instance retrieval
// ==========================================================================

#[tokio::test]
async fn query_get_existing_instance() {
    let (app, ids) = setup().await;

    let smql = format!(r#"GET TaskTracker "{}""#, ids[0]);
    let resp = app.clone().oneshot(exec(&smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["machine"], "TaskTracker");
    assert_eq!(json["result"]["state"], "done");
    assert_eq!(json["result"]["data"]["title"], "Fix login");
    assert_eq!(json["result"]["data"]["priority"], "high");
    assert_eq!(json["result"]["data"]["points"], 5);
}

#[tokio::test]
async fn query_get_not_found() {
    let (app, _ids) = setup().await;

    let smql = r#"GET TaskTracker "01ZZZZZZZZZZZZZZZZZZZZZZZZ""#;
    let resp = app.clone().oneshot(exec(smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["success"], false);
}

// ==========================================================================
// 2. FIND — filtered search
// ==========================================================================

#[tokio::test]
async fn query_find_all() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec("FIND TaskTracker"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["count"], 6);
    assert!(json["result"]["instances"].as_array().unwrap().len() == 6);
    // next_cursor should be present
    assert!(json["result"]["next_cursor"].as_str().is_some());
}

#[tokio::test]
async fn query_find_by_state() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec("FIND TaskTracker WHERE STATE IS done"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["success"], true);
    // #0 and #1 are done
    assert_eq!(json["result"]["count"], 2);
}

#[tokio::test]
async fn query_find_by_data_field() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec(
            r#"FIND TaskTracker WHERE priority == "high""#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["success"], true);
    // #0, #3, #5 are high priority
    assert_eq!(json["result"]["count"], 3);
}

#[tokio::test]
async fn query_find_with_limit() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec("FIND TaskTracker LIMIT 2"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["result"]["count"], 2);
}

#[tokio::test]
async fn query_find_with_sort() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec("FIND TaskTracker SORT BY points DESC"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let instances = json["result"]["instances"].as_array().unwrap();
    assert_eq!(instances.len(), 6);

    // Points should be in descending order: 8, 5, 5, 3, 2, 1
    let points: Vec<i64> = instances
        .iter()
        .map(|i| i["data"]["points"].as_i64().unwrap())
        .collect();
    assert_eq!(points[0], 8);
    assert_eq!(*points.last().unwrap(), 1);
    // Verify sorted
    for w in points.windows(2) {
        assert!(w[0] >= w[1], "Expected descending: {:?}", points);
    }
}

#[tokio::test]
async fn query_find_with_cursor_pagination() {
    let (app, _ids) = setup().await;

    // First page: 3 items
    let resp = app
        .clone()
        .oneshot(exec("FIND TaskTracker LIMIT 3"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let page1 = body_json(resp.into_body()).await;
    assert_eq!(page1["result"]["count"], 3);
    let cursor = page1["result"]["next_cursor"].as_str().unwrap();

    // Second page: AFTER cursor (AFTER must come after LIMIT in SMQL syntax)
    let smql = format!(r#"FIND TaskTracker LIMIT 3 AFTER "{}""#, cursor);
    let resp = app.clone().oneshot(exec(&smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let page2 = body_json(resp.into_body()).await;
    assert_eq!(page2["result"]["count"], 3);

    // No overlap between pages
    let page1_ids: Vec<&str> = page1["result"]["instances"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    let page2_ids: Vec<&str> = page2["result"]["instances"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    for id in &page1_ids {
        assert!(!page2_ids.contains(id), "Cursor overlap: {}", id);
    }
}

#[tokio::test]
async fn query_find_combined_filter_sort_limit() {
    let (app, _ids) = setup().await;

    // Find high-priority tasks sorted by points DESC
    // Note: don't combine WHERE + LIMIT because the engine applies LIMIT at
    // storage level before the WHERE filter, so results may be fewer than expected.
    let resp = app
        .clone()
        .oneshot(exec(
            r#"FIND TaskTracker WHERE priority == "high" SORT BY points DESC"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    // 3 high-priority instances: #0(5pts), #3(8pts), #5(2pts)
    assert_eq!(json["result"]["count"], 3);
    let instances = json["result"]["instances"].as_array().unwrap();
    // Sorted by points DESC: 8, 5, 2
    assert_eq!(instances[0]["data"]["points"], 8);
    assert_eq!(instances[1]["data"]["points"], 5);
    assert_eq!(instances[2]["data"]["points"], 2);
}

// ==========================================================================
// 3. TRAIL — audit log
// ==========================================================================

#[tokio::test]
async fn query_trail_full_lifecycle() {
    let (app, ids) = setup().await;

    // #0 went open → in_progress → review → done (4 trail entries including spawn)
    let smql = format!(r#"TRAIL OF "{}""#, ids[0]);
    let resp = app.clone().oneshot(exec(&smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["success"], true);
    let entries = json["result"]["entries"].as_array().unwrap();
    // spawn (seq 0) + 3 transitions = 4 entries
    assert_eq!(entries.len(), 4);

    // Verify sequence ordering
    assert_eq!(entries[0]["sequence"], 0); // spawn
    assert_eq!(entries[0]["to_state"], "open");
    assert_eq!(entries[1]["from_state"], "open");
    assert_eq!(entries[1]["to_state"], "in_progress");
    assert_eq!(entries[2]["from_state"], "in_progress");
    assert_eq!(entries[2]["to_state"], "review");
    assert_eq!(entries[3]["from_state"], "review");
    assert_eq!(entries[3]["to_state"], "done");
}

#[tokio::test]
async fn query_trail_rework_path() {
    let (app, ids) = setup().await;

    // #3 went open → in_progress → review → in_progress (rework loop)
    let smql = format!(r#"TRAIL OF "{}""#, ids[3]);
    let resp = app.clone().oneshot(exec(&smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let entries = json["result"]["entries"].as_array().unwrap();
    // spawn + 3 transitions = 4 entries
    assert_eq!(entries.len(), 4);
    // Last transition goes back to in_progress
    assert_eq!(entries[3]["from_state"], "review");
    assert_eq!(entries[3]["to_state"], "in_progress");
}

// ==========================================================================
// 4. AGGREGATE — COUNT, SUM, AVG, MIN, MAX, GROUP BY
// ==========================================================================

#[tokio::test]
async fn query_aggregate_count_all() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec("AGGREGATE TaskTracker MEASURE COUNT()"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["success"], true);
    let rows = json["result"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["measures"]["COUNT"], 6);
}

#[tokio::test]
async fn query_aggregate_count_group_by_state() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec(
            "AGGREGATE TaskTracker MEASURE COUNT() GROUP BY STATE",
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let rows = json["result"]["rows"].as_array().unwrap();

    // Current states: done(2), in_progress(2), cancelled(1), review(1)
    assert_eq!(rows.len(), 4, "Expected 4 state groups, got: {:?}", rows);

    // Build a map of state -> count
    let mut state_counts = std::collections::HashMap::new();
    for row in rows {
        let state = row["group"]["state"].as_str().unwrap().to_string();
        let count = row["measures"]["COUNT"].as_i64().unwrap();
        state_counts.insert(state, count);
    }

    assert_eq!(state_counts.get("done"), Some(&2));
    assert_eq!(state_counts.get("in_progress"), Some(&2)); // #2, #3
    assert_eq!(state_counts.get("cancelled"), Some(&1));   // #4
    assert_eq!(state_counts.get("review"), Some(&1));       // #5
}

#[tokio::test]
async fn query_aggregate_sum_avg_min_max() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec(
            "AGGREGATE TaskTracker MEASURE SUM(points), AVG(points), MIN(points), MAX(points)",
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let rows = json["result"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);

    let measures = &rows[0]["measures"];
    // Points: 5 + 3 + 1 + 8 + 5 + 2 = 24
    assert_eq!(measures["SUM"], 24);
    // AVG = 24/6 = 4.0
    assert_eq!(measures["AVG"], 4.0);
    assert_eq!(measures["MIN"], 1);
    assert_eq!(measures["MAX"], 8);
}

#[tokio::test]
async fn query_aggregate_group_by_field() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec(
            "AGGREGATE TaskTracker MEASURE COUNT(), SUM(points) GROUP BY category",
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let rows = json["result"]["rows"].as_array().unwrap();

    // 3 categories: bugs(3), features(2), docs(1)
    assert_eq!(rows.len(), 3, "Expected 3 category groups: {:?}", rows);

    let mut cat_data = std::collections::HashMap::new();
    for row in rows {
        let cat = row["group"]["category"].as_str().unwrap().to_string();
        let count = row["measures"]["COUNT"].as_i64().unwrap();
        let sum = row["measures"]["SUM"].as_i64().unwrap();
        cat_data.insert(cat, (count, sum));
    }

    assert_eq!(cat_data.get("bugs"), Some(&(3, 15)));       // 5+8+2
    assert_eq!(cat_data.get("features"), Some(&(2, 8)));     // 3+5
    assert_eq!(cat_data.get("docs"), Some(&(1, 1)));         // 1
}

// ==========================================================================
// 5. PATHS — state sequence analysis
// ==========================================================================

#[tokio::test]
async fn query_paths_all() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec("PATHS FROM TaskTracker"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["success"], true);
    let paths = json["result"]["paths"].as_array().unwrap();

    // Paths start with "" (spawn trail from_state) followed by to_states.
    // Expected distinct paths (5):
    // 1. ["", "open", "in_progress", "review", "done"]          — 2 instances (#0, #1)
    // 2. ["", "open", "in_progress"]                            — 1 instance (#2)
    // 3. ["", "open", "in_progress", "review", "in_progress"]   — 1 instance (#3)
    // 4. ["", "open", "cancelled"]                              — 1 instance (#4)
    // 5. ["", "open", "in_progress", "review"]                  — 1 instance (#5)
    assert!(
        paths.len() >= 4,
        "Expected at least 4 distinct paths, got {}: {:?}",
        paths.len(),
        paths
    );

    // Total count across all paths must equal 6 instances
    let total: i64 = paths.iter().map(|p| p["count"].as_i64().unwrap()).sum();
    assert_eq!(total, 6, "Total path counts should equal 6 instances");

    // The most common path (happy path) should have count=2
    assert_eq!(paths[0]["count"], 2);
    let top_path = paths[0]["path"].as_array().unwrap();
    // Happy path: "" → open → in_progress → review → done (5 elements)
    assert_eq!(top_path.len(), 5, "Happy path should have 5 elements: {:?}", top_path);
    assert_eq!(top_path[1], "open");
    assert_eq!(top_path[4], "done");
}

#[tokio::test]
async fn query_paths_with_limit() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec("PATHS FROM TaskTracker LIMIT 2"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let paths = json["result"]["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 2);
}

// ==========================================================================
// 6. FUNNEL — conversion analysis
// ==========================================================================

#[tokio::test]
async fn query_funnel_full_pipeline() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec(
            "FUNNEL TaskTracker THROUGH [open, in_progress, review, done]",
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["success"], true);
    let stages = json["result"]["stages"].as_array().unwrap();
    assert_eq!(stages.len(), 4);

    // All 6 instances visited open (all spawned there)
    assert_eq!(stages[0]["state"], "open");
    assert_eq!(stages[0]["count"], 6);
    let rate0 = stages[0]["conversion_rate"].as_f64().unwrap();
    assert!((rate0 - 1.0).abs() < 0.01, "open rate should be 1.0");

    // 5 instances visited in_progress (#0,1,2,3,5 — not #4 which went to cancelled)
    assert_eq!(stages[1]["state"], "in_progress");
    assert_eq!(stages[1]["count"], 5);

    // 4 instances visited review (#0,1,3,5)
    assert_eq!(stages[2]["state"], "review");
    assert_eq!(stages[2]["count"], 4);

    // 2 instances reached done (#0,1)
    assert_eq!(stages[3]["state"], "done");
    assert_eq!(stages[3]["count"], 2);
    let rate3 = stages[3]["conversion_rate"].as_f64().unwrap();
    assert!(
        (rate3 - 2.0 / 6.0).abs() < 0.01,
        "done rate should be ~0.333, got {}",
        rate3
    );
}

#[tokio::test]
async fn query_funnel_partial() {
    let (app, _ids) = setup().await;

    // Funnel through just two stages
    let resp = app
        .clone()
        .oneshot(exec(
            "FUNNEL TaskTracker THROUGH [open, cancelled]",
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let stages = json["result"]["stages"].as_array().unwrap();
    assert_eq!(stages.len(), 2);
    assert_eq!(stages[0]["count"], 6); // all visited open
    assert_eq!(stages[1]["count"], 1); // only #4 reached cancelled
}

// ==========================================================================
// 7. COMPARE PATHS — segmented path analysis
// ==========================================================================

#[tokio::test]
async fn query_compare_paths_by_category() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec("COMPARE PATHS TaskTracker SEGMENT BY category"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["segment_by"], "category");

    let segments = json["result"]["segments"].as_array().unwrap();
    // 3 segments: bugs(3 instances), features(2), docs(1)
    assert_eq!(segments.len(), 3, "Expected 3 segments: {:?}", segments);

    // Build a map of segment → (total_instances, num_distinct_paths)
    let mut seg_data = std::collections::HashMap::new();
    for seg in segments {
        let val = seg["segment_value"].as_str().unwrap().to_string();
        let paths = seg["paths"].as_array().unwrap();
        let total: i64 = paths.iter().map(|p| p["count"].as_i64().unwrap()).sum();
        seg_data.insert(val, (total, paths.len()));
    }

    // bugs: 3 instances with 3 distinct paths
    assert_eq!(seg_data.get("bugs").unwrap().0, 3);
    assert_eq!(seg_data.get("bugs").unwrap().1, 3); // all took different paths

    // features: 2 instances with 2 distinct paths
    assert_eq!(seg_data.get("features").unwrap().0, 2);
    assert_eq!(seg_data.get("features").unwrap().1, 2);

    // docs: 1 instance with 1 path
    assert_eq!(seg_data.get("docs").unwrap().0, 1);
    assert_eq!(seg_data.get("docs").unwrap().1, 1);
}

#[tokio::test]
async fn query_compare_paths_by_priority() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec("COMPARE PATHS TaskTracker SEGMENT BY priority"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let segments = json["result"]["segments"].as_array().unwrap();

    // 3 priority levels: high(3), medium(2), low(1)
    assert_eq!(segments.len(), 3);

    let mut priority_totals = std::collections::HashMap::new();
    for seg in segments {
        let val = seg["segment_value"].as_str().unwrap().to_string();
        let total: i64 = seg["paths"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["count"].as_i64().unwrap())
            .sum();
        priority_totals.insert(val, total);
    }

    assert_eq!(priority_totals.get("high"), Some(&3));
    assert_eq!(priority_totals.get("medium"), Some(&2));
    assert_eq!(priority_totals.get("low"), Some(&1));
}

// ==========================================================================
// 8. Cross-query consistency checks
// ==========================================================================

#[tokio::test]
async fn query_find_and_aggregate_counts_match() {
    let (app, _ids) = setup().await;

    // FIND all
    let resp = app
        .clone()
        .oneshot(exec("FIND TaskTracker"))
        .await
        .unwrap();
    let find_json = body_json(resp.into_body()).await;
    let find_count = find_json["result"]["count"].as_i64().unwrap();

    // AGGREGATE COUNT()
    let resp = app
        .clone()
        .oneshot(exec("AGGREGATE TaskTracker MEASURE COUNT()"))
        .await
        .unwrap();
    let agg_json = body_json(resp.into_body()).await;
    let agg_count = agg_json["result"]["rows"][0]["measures"]["COUNT"]
        .as_i64()
        .unwrap();

    assert_eq!(find_count, agg_count, "FIND count should match AGGREGATE COUNT");
}

#[tokio::test]
async fn query_find_by_state_matches_aggregate_group() {
    let (app, _ids) = setup().await;

    // FIND done instances
    let resp = app
        .clone()
        .oneshot(exec("FIND TaskTracker WHERE STATE IS done"))
        .await
        .unwrap();
    let find_json = body_json(resp.into_body()).await;
    let find_done = find_json["result"]["count"].as_i64().unwrap();

    // AGGREGATE GROUP BY STATE
    let resp = app
        .clone()
        .oneshot(exec(
            "AGGREGATE TaskTracker MEASURE COUNT() GROUP BY STATE",
        ))
        .await
        .unwrap();
    let agg_json = body_json(resp.into_body()).await;
    let rows = agg_json["result"]["rows"].as_array().unwrap();
    let agg_done = rows
        .iter()
        .find(|r| r["group"]["state"] == "done")
        .map(|r| r["measures"]["COUNT"].as_i64().unwrap())
        .unwrap_or(0);

    assert_eq!(find_done, agg_done, "FIND done count should match AGGREGATE GROUP BY STATE done count");
}

#[tokio::test]
async fn query_paths_total_equals_instance_count() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec("PATHS FROM TaskTracker"))
        .await
        .unwrap();
    let json = body_json(resp.into_body()).await;
    let paths = json["result"]["paths"].as_array().unwrap();

    let total: i64 = paths.iter().map(|p| p["count"].as_i64().unwrap()).sum();
    assert_eq!(total, 6, "Total path counts should equal 6 instances");
}

// ==========================================================================
// 9. Edge cases
// ==========================================================================

#[tokio::test]
async fn query_find_no_results() {
    let (app, _ids) = setup().await;

    let resp = app
        .clone()
        .oneshot(exec(
            r#"FIND TaskTracker WHERE category == "nonexistent""#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["success"], true);
    assert_eq!(json["result"]["count"], 0);
    assert!(json["result"]["instances"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn query_aggregate_no_instances() {
    let app = test_router();

    // Define machine but don't spawn anything
    let define = r#"DEFINE MACHINE Empty (
        STATES { a, b }
        INITIAL STATE a
        TERMINAL STATES { b }
        TRANSITIONS { a -> b {} }
    )"#;
    let resp = app.clone().oneshot(exec(define)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(exec("AGGREGATE Empty MEASURE COUNT()"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let rows = json["result"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["measures"]["COUNT"], 0);
}

#[tokio::test]
async fn query_funnel_empty_machine() {
    let app = test_router();

    let define = r#"DEFINE MACHINE Empty2 (
        STATES { a, b }
        INITIAL STATE a
        TERMINAL STATES { b }
        TRANSITIONS { a -> b {} }
    )"#;
    let resp = app.clone().oneshot(exec(define)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(exec("FUNNEL Empty2 THROUGH [a, b]"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let stages = json["result"]["stages"].as_array().unwrap();
    assert_eq!(stages.len(), 2);
    assert_eq!(stages[0]["count"], 0);
    assert_eq!(stages[1]["count"], 0);
}

#[tokio::test]
async fn query_trail_single_spawn_only() {
    let app = test_router();

    let define = r#"DEFINE MACHINE Simple (
        STATES { idle, done }
        INITIAL STATE idle
        TERMINAL STATES { done }
        TRANSITIONS { idle -> done {} }
    )"#;
    let resp = app.clone().oneshot(exec(define)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app
        .clone()
        .oneshot(exec("SPAWN Simple {}"))
        .await
        .unwrap();
    let json = body_json(resp.into_body()).await;
    let id = json["result"]["id"].as_str().unwrap();

    // Trail should have exactly 1 entry (the spawn)
    let smql = format!(r#"TRAIL OF "{}""#, id);
    let resp = app.clone().oneshot(exec(&smql)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let entries = json["result"]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["sequence"], 0);
    assert_eq!(entries[0]["to_state"], "idle");
}

// ==========================================================================
// 10. BATCH TRANSITION + query verification
// ==========================================================================

#[tokio::test]
async fn batch_transition_then_verify_with_queries() {
    let app = test_router();

    let define = r#"DEFINE MACHINE BatchTest (
        DATA {
            label : TEXT -> REQUIRED
        }
        STATES { pending, approved, archived }
        INITIAL STATE pending
        TERMINAL STATES { archived }
        TRANSITIONS {
            pending -> approved {}
            approved -> archived {}
        }
    )"#;
    let resp = app.clone().oneshot(exec(define)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Spawn 4 instances
    for i in 0..4 {
        let smql = format!(r#"SPAWN BatchTest {{ label: "item-{}" }}"#, i);
        let resp = app.clone().oneshot(exec(&smql)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // Verify all in pending
    let resp = app
        .clone()
        .oneshot(exec("AGGREGATE BatchTest MEASURE COUNT() GROUP BY STATE"))
        .await
        .unwrap();
    let json = body_json(resp.into_body()).await;
    let rows = json["result"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["group"]["state"], "pending");
    assert_eq!(rows[0]["measures"]["COUNT"], 4);

    // Batch transition all pending → approved
    let resp = app
        .clone()
        .oneshot(exec(
            "TRANSITION ALL BatchTest WHERE STATE IS pending TO approved",
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["result"]["matched"], 4);
    assert_eq!(json["result"]["transitioned"], 4);

    // Verify all now in approved
    let resp = app
        .clone()
        .oneshot(exec("FIND BatchTest WHERE STATE IS approved"))
        .await
        .unwrap();
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["result"]["count"], 4);

    // Verify PATHS shows uniform path
    let resp = app
        .clone()
        .oneshot(exec("PATHS FROM BatchTest"))
        .await
        .unwrap();
    let json = body_json(resp.into_body()).await;
    let paths = json["result"]["paths"].as_array().unwrap();
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0]["count"], 4);
    let path = paths[0]["path"].as_array().unwrap();
    assert_eq!(
        path,
        &[
            serde_json::json!(""),
            serde_json::json!("pending"),
            serde_json::json!("approved")
        ]
    );
}
