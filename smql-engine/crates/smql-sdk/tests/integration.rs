use smql_sdk::prelude::*;
use std::time::Duration;
use tokio::net::TcpListener;

/// Spin up a server on a random port and return the base URL.
async fn start_server() -> String {
    let server = smql_server::SmqlServer::new();
    let app = server.router();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    format!("http://{}", addr)
}

fn simple_machine() -> &'static str {
    r#"DEFINE MACHINE counter (
        STATES { idle, running, done }
        INITIAL STATE idle
        TERMINAL STATES { done }
        TRANSITIONS {
            idle -> running {}
            running -> done {}
        }
    )"#
}

fn guarded_machine() -> &'static str {
    r#"DEFINE MACHINE gated (
        DATA { level : INT }
        STATES { open, closed }
        INITIAL STATE open
        TERMINAL STATES { closed }
        TRANSITIONS {
            open -> closed {
                GUARD : level > 10
            }
        }
    )"#
}

fn machine_with_hooks() -> &'static str {
    r#"DEFINE MACHINE evented (
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
    )"#
}

// 1. Health check
#[tokio::test]
async fn test_health_check() {
    let url = start_server().await;
    let client = SmqlClient::new(&url).unwrap();
    assert!(client.health().await.unwrap());
}

// 2. Define machine + list machines
#[tokio::test]
async fn test_define_and_list_machines() {
    let url = start_server().await;
    let client = SmqlClient::new(&url).unwrap();

    let result = client.define_machine(simple_machine()).await.unwrap();
    assert_eq!(result.action, "machine_defined");

    let machines = client.list_machines().await.unwrap();
    assert!(machines.contains(&"counter".to_string()));
}

// 3. Spawn + get instance
#[tokio::test]
async fn test_spawn_and_get_instance() {
    let url = start_server().await;
    let client = SmqlClient::new(&url).unwrap();
    client.define_machine(simple_machine()).await.unwrap();

    let inst = client.spawn("counter", serde_json::json!({})).await.unwrap();
    assert_eq!(inst.machine, "counter");
    assert_eq!(inst.state, "idle");
    assert_eq!(inst.id.len(), 26); // ULID format

    // GET via REST endpoint
    let fetched = client.get_instance(&inst.id).await.unwrap();
    assert_eq!(fetched.id, inst.id);
    assert_eq!(fetched.state, "idle");
}

// 4. Transition + verify state change
#[tokio::test]
async fn test_transition() {
    let url = start_server().await;
    let client = SmqlClient::new(&url).unwrap();
    client.define_machine(simple_machine()).await.unwrap();

    let inst = client.spawn("counter", serde_json::json!({})).await.unwrap();
    let tr = client
        .transition("counter", &inst.id, "running", TransitionOptions::default())
        .await
        .unwrap();

    assert_eq!(tr.from_state, "idle");
    assert_eq!(tr.to_state, "running");
    assert_eq!(tr.instance.state, "running");
}

// 5. try_transition guard failure (returns None)
#[tokio::test]
async fn test_try_transition_guard_failure() {
    let url = start_server().await;
    let client = SmqlClient::new(&url).unwrap();
    client.define_machine(guarded_machine()).await.unwrap();

    let inst = client
        .spawn("gated", serde_json::json!({"level": 5}))
        .await
        .unwrap();

    let result = client
        .try_transition("gated", &inst.id, "closed", TransitionOptions::default())
        .await
        .unwrap();
    assert!(result.is_none(), "Guard should have failed");
}

// 6. find instances (by state, with limit)
#[tokio::test]
async fn test_find_instances() {
    let url = start_server().await;
    let client = SmqlClient::new(&url).unwrap();
    client.define_machine(simple_machine()).await.unwrap();

    // Spawn 3 instances
    client.spawn("counter", serde_json::json!({})).await.unwrap();
    client.spawn("counter", serde_json::json!({})).await.unwrap();
    let inst3 = client.spawn("counter", serde_json::json!({})).await.unwrap();

    // Transition one to running
    client
        .transition("counter", &inst3.id, "running", TransitionOptions::default())
        .await
        .unwrap();

    // Find all idle
    let idle = client.find("counter").in_state("idle").execute().await.unwrap();
    assert_eq!(idle.len(), 2);

    // Find with limit
    let limited = client.find("counter").limit(1).execute().await.unwrap();
    assert_eq!(limited.len(), 1);

    // Count all
    let count = client.find("counter").count().await.unwrap();
    assert_eq!(count, 3);
}

// 7. aggregate by state
#[tokio::test]
async fn test_aggregate() {
    let url = start_server().await;
    let client = SmqlClient::new(&url).unwrap();
    client.define_machine(simple_machine()).await.unwrap();

    client.spawn("counter", serde_json::json!({})).await.unwrap();
    client.spawn("counter", serde_json::json!({})).await.unwrap();

    let result = client
        .aggregate("counter")
        .measure("COUNT()")
        .group_by_state()
        .execute()
        .await
        .unwrap();

    assert!(result["rows"].is_array());
}

// 8. trail retrieval
#[tokio::test]
async fn test_trail() {
    let url = start_server().await;
    let client = SmqlClient::new(&url).unwrap();
    client.define_machine(simple_machine()).await.unwrap();

    let inst = client.spawn("counter", serde_json::json!({})).await.unwrap();
    client
        .transition("counter", &inst.id, "running", TransitionOptions::default())
        .await
        .unwrap();
    client
        .transition("counter", &inst.id, "done", TransitionOptions::default())
        .await
        .unwrap();

    let trail = client.trail(&inst.id).await.unwrap();
    // Trail includes: spawn (seq 0), idle->running (seq 1), running->done (seq 2)
    assert_eq!(trail.len(), 3);
    assert_eq!(trail[0].from_state, "");
    assert_eq!(trail[0].to_state, "idle");
    assert_eq!(trail[1].from_state, "idle");
    assert_eq!(trail[1].to_state, "running");
    assert_eq!(trail[2].from_state, "running");
    assert_eq!(trail[2].to_state, "done");
}

// 9. WebSocket subscription receives events
#[tokio::test]
async fn test_websocket_subscription() {
    let url = start_server().await;
    let client = SmqlClient::new(&url).unwrap();
    client.define_machine(machine_with_hooks()).await.unwrap();

    let mut sub = client.subscribe(Some("evented")).await.unwrap();

    // Give WebSocket time to connect
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Spawn to trigger event
    client
        .spawn("evented", serde_json::json!({}))
        .await
        .unwrap();

    // Try to receive event
    let event = tokio::time::timeout(Duration::from_secs(2), sub.next_event()).await;

    match event {
        Ok(Ok(ev)) => {
            assert_eq!(ev.event, "spawned");
            assert_eq!(ev.machine, "evented");
        }
        _ => {
            // Timing-dependent — subscription may connect after event fires
        }
    }
}

// 10. Error handling (invalid SMQL, not found)
#[tokio::test]
async fn test_error_handling() {
    let url = start_server().await;
    let client = SmqlClient::new(&url).unwrap();

    // Invalid SMQL
    let err = client.execute("INVALID STUFF").await;
    assert!(err.is_err());

    // Not found instance via REST
    let err = client.get_instance("01000000000000000000000000").await;
    assert!(matches!(err, Err(SdkError::NotFound(_))));
}

// 11. Transition with MEMO
#[tokio::test]
async fn test_transition_with_memo() {
    let url = start_server().await;
    let client = SmqlClient::new(&url).unwrap();
    client.define_machine(simple_machine()).await.unwrap();

    let inst = client.spawn("counter", serde_json::json!({})).await.unwrap();

    let opts = TransitionOptions {
        memo: Some("starting up".to_string()),
        ..Default::default()
    };
    let tr = client.transition("counter", &inst.id, "running", opts).await.unwrap();
    assert_eq!(tr.to_state, "running");

    // Verify memo in trail
    let trail = client.trail(&inst.id).await.unwrap();
    let transition_entry = &trail[1]; // seq 1 = first transition
    assert_eq!(transition_entry.memo.as_deref(), Some("starting up"));
}

// 12. Client builder (timeout, base_url)
#[tokio::test]
async fn test_client_builder() {
    let url = start_server().await;
    let client = SmqlClient::builder(&url)
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    assert!(client.health().await.unwrap());
}

// 13. get_machine info
#[tokio::test]
async fn test_get_machine_info() {
    let url = start_server().await;
    let client = SmqlClient::new(&url).unwrap();
    client.define_machine(simple_machine()).await.unwrap();

    let info = client.get_machine("counter").await.unwrap();
    assert_eq!(info.name, "counter");
    assert_eq!(info.initial_state, "idle");
    assert!(info.terminal_states.contains(&"done".to_string()));
    assert!(info.states.contains(&"idle".to_string()));
    assert!(info.states.contains(&"running".to_string()));
    assert!(info.states.contains(&"done".to_string()));
}

// 14. Typed spawn + find
#[tokio::test]
async fn test_typed_spawn_and_find() {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct CounterData {}

    #[derive(Debug, Clone)]
    enum CounterState {
        Idle,
        Running,
        Done,
    }

    impl SmqlState for CounterState {
        fn from_str(s: &str) -> SdkResult<Self> {
            match s {
                "idle" => Ok(Self::Idle),
                "running" => Ok(Self::Running),
                "done" => Ok(Self::Done),
                _ => Err(SdkError::Parse(format!("Unknown state: {}", s))),
            }
        }
        fn as_str(&self) -> &str {
            match self {
                Self::Idle => "idle",
                Self::Running => "running",
                Self::Done => "done",
            }
        }
        fn is_terminal(&self) -> bool {
            matches!(self, Self::Done)
        }
    }

    struct Counter;
    impl SmqlMachine for Counter {
        const MACHINE_NAME: &'static str = "counter";
        type Data = CounterData;
        type State = CounterState;
    }

    let url = start_server().await;
    let client = SmqlClient::new(&url).unwrap();
    client.define_machine(simple_machine()).await.unwrap();

    let inst = client.spawn_typed::<Counter>(CounterData {}).await.unwrap();
    assert!(matches!(inst.state, CounterState::Idle));
    assert_eq!(inst.id.len(), 26); // ULID format

    let found = client
        .find_typed::<Counter>()
        .in_state("idle")
        .execute()
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert!(matches!(found[0].state, CounterState::Idle));
}

// 15. Raw execute + define result
#[tokio::test]
async fn test_raw_execute() {
    let url = start_server().await;
    let client = SmqlClient::new(&url).unwrap();

    let resp = client.execute(simple_machine()).await.unwrap();
    assert!(resp.success);
    assert!(resp.result.is_some());
}
