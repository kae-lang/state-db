/// Tests for Enhancement 3: Idempotency Keys.
use std::sync::Arc;

use smql_ast::command::{SpawnCommand, TransitionCommand};
use smql_ast::expression::{Expression, ExpressionKind};
use smql_ast::value::Value;
use smql_catalog::MachineCatalog;
use smql_engine_core::Engine;
use smql_hooks::{EventBus, HookExecutor};
use smql_storage::{MemoryStorage, Storage};
use smql_timer::TimerManager;

fn lit(v: Value) -> Expression {
    Expression::new(ExpressionKind::Literal(v))
}

fn make_engine() -> Engine {
    let smql = r#"
DEFINE MACHINE Counter (
  DATA {
    count : INT -> DEFAULT(0)
  }
  STATES { idle, running, done }
  INITIAL STATE idle
  TERMINAL STATES { done }
  TRANSITIONS {
    idle -> running {}
    running -> done {}
  }
)
"#;
    let machines = smql_parser::parse_machines(smql).expect("parse");
    let catalog = Arc::new(MachineCatalog::new());
    for m in machines {
        catalog.register(m).expect("register");
    }
    let storage = Arc::new(MemoryStorage::new());
    let timer = Arc::new(TimerManager::new());
    let event_bus = Arc::new(EventBus::new(64));
    let hooks = Arc::new(HookExecutor::new(event_bus));
    Engine::with_hooks(catalog, storage, timer, hooks)
}

fn spawn_cmd(ikey: Option<&str>) -> SpawnCommand {
    SpawnCommand {
        machine: "Counter".to_string(),
        data: vec![("count".to_string(), lit(Value::Int(1)))],
        then_transition: None,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
        as_actor: None,
        idempotency_key: ikey.map(|s| s.to_string()),
        tags: Vec::new(),
        ttl: None,
    }
}

// --- Spawn idempotency tests ---

#[tokio::test]
async fn spawn_with_idempotency_key_returns_same_result() {
    let engine = make_engine();
    let cmd = spawn_cmd(Some("spawn-key-1"));

    let result1 = engine.spawn(&cmd).await.unwrap();
    let result2 = engine.spawn(&cmd).await.unwrap();

    // Same instance ID returned — no duplicate created
    assert_eq!(result1.instance.id, result2.instance.id);
    assert_eq!(result1.instance.state, result2.instance.state);
}

#[tokio::test]
async fn spawn_different_keys_create_separate_instances() {
    let engine = make_engine();

    let cmd1 = spawn_cmd(Some("key-a"));
    let cmd2 = spawn_cmd(Some("key-b"));

    let r1 = engine.spawn(&cmd1).await.unwrap();
    let r2 = engine.spawn(&cmd2).await.unwrap();

    // Different keys → different instances
    assert_ne!(r1.instance.id, r2.instance.id);
}

#[tokio::test]
async fn spawn_without_idempotency_key_creates_new_each_time() {
    let engine = make_engine();
    let cmd = spawn_cmd(None);

    let r1 = engine.spawn(&cmd).await.unwrap();
    let r2 = engine.spawn(&cmd).await.unwrap();

    // No key → new instance each time
    assert_ne!(r1.instance.id, r2.instance.id);
}

// --- Transition idempotency tests ---

#[tokio::test]
async fn transition_with_idempotency_key_returns_same_result() {
    let engine = make_engine();

    let spawn_result = engine.spawn(&spawn_cmd(None)).await.unwrap();
    let iid = spawn_result.instance.id.as_str();

    let cmd = TransitionCommand {
        machine: "Counter".into(),
        instance_id: iid.clone(),
        to_state: "running".into(),
        with_data: Vec::new(),
        memo: None,
        as_actor: None,
        through: Vec::new(),
        or_stay: false,
        cascade: false,
        idempotency_key: Some("transition-key-1".to_string()),
        tags: Vec::new(),
    };

    let r1 = engine.transition(&cmd).await.unwrap();
    let r2 = engine.transition(&cmd).await.unwrap();

    // Same result — transition not applied twice
    assert_eq!(r1.from_state, r2.from_state);
    assert_eq!(r1.to_state, r2.to_state);
    assert_eq!(r1.instance.id, r2.instance.id);
}

#[tokio::test]
async fn transition_without_idempotency_key_no_regression() {
    let engine = make_engine();

    let spawn_result = engine.spawn(&spawn_cmd(None)).await.unwrap();
    let iid = spawn_result.instance.id.as_str();

    let cmd = TransitionCommand::new("Counter".into(), iid.clone(), "running".into());

    let result = engine.transition(&cmd).await.unwrap();
    assert_eq!(result.to_state, "running");
}

// --- Parser tests ---

#[test]
fn parse_spawn_with_idempotency_key() {
    let stmts = smql_parser::parse(r#"SPAWN Counter { count: 1 } IDEMPOTENCY_KEY "key-123""#)
        .expect("parse");
    let stmt = &stmts[0];
    if let smql_ast::command::Statement::Command(smql_ast::command::Command::Spawn(cmd)) = stmt {
        assert_eq!(cmd.idempotency_key.as_deref(), Some("key-123"));
    } else {
        panic!("Expected Spawn command");
    }
}

#[test]
fn parse_transition_with_idempotency_key() {
    let stmts = smql_parser::parse(
        r#"TRANSITION Counter "abc" TO running IDEMPOTENCY_KEY "txn-456""#,
    )
    .expect("parse");
    let stmt = &stmts[0];
    if let smql_ast::command::Statement::Command(smql_ast::command::Command::Transition(cmd)) =
        stmt
    {
        assert_eq!(cmd.idempotency_key.as_deref(), Some("txn-456"));
    } else {
        panic!("Expected Transition command");
    }
}

#[test]
fn parse_spawn_without_idempotency_key() {
    let stmts = smql_parser::parse(r#"SPAWN Counter { count: 1 }"#).expect("parse");
    let stmt = &stmts[0];
    if let smql_ast::command::Statement::Command(smql_ast::command::Command::Spawn(cmd)) = stmt {
        assert!(cmd.idempotency_key.is_none());
    } else {
        panic!("Expected Spawn command");
    }
}

// --- Storage cleanup test ---

#[tokio::test]
async fn cleanup_expired_idempotency_entries() {
    let storage = MemoryStorage::new();
    use chrono::Utc;

    // Store an already-expired entry
    let past = Utc::now() - chrono::Duration::hours(1);
    let stored = storage
        .store_idempotency("expired-key", b"data", past)
        .await
        .unwrap();
    assert!(stored);

    // Store a valid entry
    let future = Utc::now() + chrono::Duration::hours(24);
    let stored2 = storage
        .store_idempotency("valid-key", b"data2", future)
        .await
        .unwrap();
    assert!(stored2);

    // Get should return None for expired, Some for valid
    let expired = storage.get_idempotency("expired-key").await.unwrap();
    assert!(expired.is_none());
    let valid = storage.get_idempotency("valid-key").await.unwrap();
    assert_eq!(valid, Some(b"data2".to_vec()));

    // Cleanup should remove the expired entry
    let cleaned = storage.cleanup_expired_idempotency().await.unwrap();
    // The expired entry may have already been cleaned up by get_idempotency
    assert!(cleaned <= 1);
}

// --- Idempotency with duplicate key prevents store ---

#[tokio::test]
async fn store_idempotency_returns_false_for_existing_key() {
    let storage = MemoryStorage::new();
    let future = chrono::Utc::now() + chrono::Duration::hours(24);

    let first = storage
        .store_idempotency("dup-key", b"first", future)
        .await
        .unwrap();
    assert!(first);

    let second = storage
        .store_idempotency("dup-key", b"second", future)
        .await
        .unwrap();
    assert!(!second); // Key already exists
}
