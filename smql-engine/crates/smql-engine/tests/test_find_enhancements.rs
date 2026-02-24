/// Tests for Enhancement 8 (FIND with Field Projection) and Enhancement 10 (Missing Query Predicates).
use smql_ast::command::{Command, Statement};
use smql_ast::query::{FindQuery, Query};
use smql_catalog::MachineCatalog;
use smql_engine_core::Engine;
use smql_storage::MemoryStorage;
use std::sync::Arc;

/// Helper: define a support ticket machine with terminal states.
const TICKET_MACHINE: &str = r#"
DEFINE MACHINE Ticket (
    DATA {
        title      : TEXT -> REQUIRED
        priority   : TEXT -> OPTIONAL
        assignee   : TEXT -> OPTIONAL
    }
    STATES { open, in_progress, resolved, closed, cancelled }
    INITIAL STATE open
    TERMINAL STATES { closed, cancelled }
    TRANSITIONS {
        open -> in_progress {}
        in_progress -> resolved {}
        resolved -> closed {}
        open -> cancelled {}
        in_progress -> cancelled {}
    }
)
"#;

async fn setup_engine() -> Arc<Engine> {
    let machines = smql_parser::parse_machines(TICKET_MACHINE).expect("parse Ticket machine");
    let catalog = Arc::new(MachineCatalog::new());
    for m in machines {
        catalog.register(m).expect("register machine");
    }
    let storage = Arc::new(MemoryStorage::new());
    let engine = Engine::new(catalog, storage);
    engine.wire_callback();
    Arc::new(engine)
}

async fn spawn_ticket(engine: &Engine, title: &str, priority: &str) -> String {
    let smql = format!(
        r#"SPAWN Ticket {{ title: "{}", priority: "{}" }}"#,
        title, priority
    );
    let stmts = smql_parser::parse(&smql).unwrap();
    if let Statement::Command(Command::Spawn(cmd)) = &stmts[0] {
        let result = engine.spawn(cmd).await.unwrap();
        result.instance.id.as_str()
    } else {
        panic!("Expected spawn command");
    }
}

async fn transition_to(engine: &Engine, id: &str, to_state: &str) {
    let smql = format!(r#"TRANSITION Ticket "{}" TO {}"#, id, to_state);
    let stmts = smql_parser::parse(&smql).unwrap();
    if let Statement::Command(Command::Transition(cmd)) = &stmts[0] {
        engine.transition(cmd).await.unwrap();
    }
}

// =============================================================================
// Enhancement 8: FIND with Field Projection
// =============================================================================

#[tokio::test]
async fn find_select_specific_fields() {
    let engine = setup_engine().await;
    spawn_ticket(&engine, "Bug 1", "high").await;
    spawn_ticket(&engine, "Bug 2", "low").await;

    let query = Query::Find(FindQuery {
        machine: "Ticket".into(),
        select: Some(vec!["title".into()]),
        filter: None,
        sort: vec![],
        limit: None,
        offset: None,
        after: None,
        as_actor: None,
    });

    let result = engine.execute_query(&query).await.unwrap();
    if let smql_engine_core::query::QueryResult::Instances(insts) = result {
        assert_eq!(insts.len(), 2);
        for inst in &insts {
            // Only "title" should be present in data (projection)
            assert!(inst.data.contains_key("title"), "should have title");
            assert!(
                !inst.data.contains_key("priority"),
                "priority should be projected out"
            );
            assert!(
                !inst.data.contains_key("assignee"),
                "assignee should be projected out"
            );
        }
    } else {
        panic!("Expected Instances result");
    }
}

#[tokio::test]
async fn find_select_with_where() {
    let engine = setup_engine().await;
    spawn_ticket(&engine, "High Bug", "high").await;
    spawn_ticket(&engine, "Low Bug", "low").await;

    let query_str = r#"FIND Ticket SELECT title WHERE priority == "high""#;
    let stmts = smql_parser::parse(query_str).unwrap();
    if let Statement::Query(query) = &stmts[0] {
        let result = engine.execute_query(query).await.unwrap();
        if let smql_engine_core::query::QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 1);
            assert_eq!(
                insts[0].data.get("title").unwrap(),
                &smql_ast::value::Value::Text("High Bug".into())
            );
            assert!(
                !insts[0].data.contains_key("priority"),
                "priority projected out"
            );
        } else {
            panic!("Expected Instances");
        }
    }
}

#[tokio::test]
async fn find_no_select_returns_all_fields() {
    let engine = setup_engine().await;
    spawn_ticket(&engine, "Test", "medium").await;

    let query = Query::Find(FindQuery {
        machine: "Ticket".into(),
        select: None,
        filter: None,
        sort: vec![],
        limit: None,
        offset: None,
        after: None,
        as_actor: None,
    });

    let result = engine.execute_query(&query).await.unwrap();
    if let smql_engine_core::query::QueryResult::Instances(insts) = result {
        assert_eq!(insts.len(), 1);
        assert!(insts[0].data.contains_key("title"));
        assert!(insts[0].data.contains_key("priority"));
    } else {
        panic!("Expected Instances result");
    }
}

// =============================================================================
// Enhancement 8: Parser tests for SELECT
// =============================================================================

#[test]
fn parse_find_with_select() {
    let stmts = smql_parser::parse(r#"FIND Ticket SELECT title, priority"#).unwrap();
    if let Statement::Query(Query::Find(q)) = &stmts[0] {
        assert_eq!(q.machine, "Ticket");
        assert_eq!(q.select, Some(vec!["title".into(), "priority".into()]));
        assert!(q.filter.is_none());
    } else {
        panic!("Expected FIND query");
    }
}

#[test]
fn parse_find_with_select_and_where() {
    let stmts =
        smql_parser::parse(r#"FIND Ticket SELECT title WHERE priority == "high""#).unwrap();
    if let Statement::Query(Query::Find(q)) = &stmts[0] {
        assert_eq!(q.select, Some(vec!["title".into()]));
        assert!(q.filter.is_some());
    } else {
        panic!("Expected FIND query");
    }
}

#[test]
fn parse_find_without_select() {
    let stmts = smql_parser::parse(r#"FIND Ticket WHERE STATE IS open"#).unwrap();
    if let Statement::Query(Query::Find(q)) = &stmts[0] {
        assert!(q.select.is_none());
        assert!(q.filter.is_some());
    } else {
        panic!("Expected FIND query");
    }
}

// =============================================================================
// Enhancement 10: ALIVE / TERMINATED predicates
// =============================================================================

#[tokio::test]
async fn find_alive_predicate() {
    let engine = setup_engine().await;
    let id1 = spawn_ticket(&engine, "Active", "high").await;
    let id2 = spawn_ticket(&engine, "Done", "low").await;

    // Close id2
    transition_to(&engine, &id2, "in_progress").await;
    transition_to(&engine, &id2, "resolved").await;
    transition_to(&engine, &id2, "closed").await;

    let stmts = smql_parser::parse("FIND Ticket WHERE ALIVE").unwrap();
    if let Statement::Query(query) = &stmts[0] {
        let result = engine.execute_query(query).await.unwrap();
        if let smql_engine_core::query::QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 1);
            assert_eq!(insts[0].id.as_str(), id1);
        } else {
            panic!("Expected Instances");
        }
    }
}

#[tokio::test]
async fn find_terminated_predicate() {
    let engine = setup_engine().await;
    spawn_ticket(&engine, "Active", "high").await;
    let id2 = spawn_ticket(&engine, "Cancelled", "low").await;

    // Cancel id2
    transition_to(&engine, &id2, "cancelled").await;

    let stmts = smql_parser::parse("FIND Ticket WHERE TERMINATED").unwrap();
    if let Statement::Query(query) = &stmts[0] {
        let result = engine.execute_query(query).await.unwrap();
        if let smql_engine_core::query::QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 1);
            assert_eq!(insts[0].id.as_str(), id2);
        } else {
            panic!("Expected Instances");
        }
    }
}

// =============================================================================
// Enhancement 10: HAS_VISITED / NEVER_VISITED predicates
// =============================================================================

#[tokio::test]
async fn find_has_visited_predicate() {
    let engine = setup_engine().await;
    let id1 = spawn_ticket(&engine, "Ticket A", "high").await;
    let id2 = spawn_ticket(&engine, "Ticket B", "low").await;

    // Move id1 through in_progress → resolved → closed
    transition_to(&engine, &id1, "in_progress").await;
    transition_to(&engine, &id1, "resolved").await;
    transition_to(&engine, &id1, "closed").await;

    // id2 stays in open
    let stmts = smql_parser::parse(r#"FIND Ticket WHERE HAS_VISITED("in_progress")"#).unwrap();
    if let Statement::Query(query) = &stmts[0] {
        let result = engine.execute_query(query).await.unwrap();
        if let smql_engine_core::query::QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 1);
            assert_eq!(insts[0].id.as_str(), id1);
        } else {
            panic!("Expected Instances");
        }
    }
}

#[tokio::test]
async fn find_never_visited_predicate() {
    let engine = setup_engine().await;
    let id1 = spawn_ticket(&engine, "Ticket A", "high").await;
    let id2 = spawn_ticket(&engine, "Ticket B", "low").await;

    // Move id1 to in_progress
    transition_to(&engine, &id1, "in_progress").await;

    // id2 stays in open — never visited in_progress
    let stmts = smql_parser::parse(r#"FIND Ticket WHERE NEVER_VISITED("in_progress")"#).unwrap();
    if let Statement::Query(query) = &stmts[0] {
        let result = engine.execute_query(query).await.unwrap();
        if let smql_engine_core::query::QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 1);
            assert_eq!(insts[0].id.as_str(), id2);
        } else {
            panic!("Expected Instances");
        }
    }
}

// =============================================================================
// Enhancement 10: STUCK_IN predicate
// =============================================================================

#[tokio::test]
async fn find_stuck_in_predicate() {
    let engine = setup_engine().await;
    let id1 = spawn_ticket(&engine, "Old Ticket", "high").await;
    spawn_ticket(&engine, "New Ticket", "low").await;

    // Note: STUCK_IN checks against state_entered_at, which is set to now() on spawn.
    // We can't easily manipulate time in tests, so we test with a very short duration
    // that both should match (since they're in "open" state).
    // The key test is that STUCK_IN parses and evaluates correctly.

    // Use 0s threshold — all tickets in "open" state are "stuck" for 0 seconds
    let stmts = smql_parser::parse(r#"FIND Ticket WHERE STUCK_IN("open", 0s)"#).unwrap();
    if let Statement::Query(query) = &stmts[0] {
        let result = engine.execute_query(query).await.unwrap();
        if let smql_engine_core::query::QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 2, "Both tickets are in 'open' for >= 0s");
        } else {
            panic!("Expected Instances");
        }
    }
}

#[tokio::test]
async fn find_stuck_in_wrong_state() {
    let engine = setup_engine().await;
    spawn_ticket(&engine, "Ticket", "high").await;

    // Looking for stuck in "in_progress" but ticket is in "open"
    let stmts = smql_parser::parse(r#"FIND Ticket WHERE STUCK_IN("in_progress", 0s)"#).unwrap();
    if let Statement::Query(query) = &stmts[0] {
        let result = engine.execute_query(query).await.unwrap();
        if let smql_engine_core::query::QueryResult::Instances(insts) = result {
            assert_eq!(
                insts.len(),
                0,
                "No tickets are in 'in_progress' state"
            );
        } else {
            panic!("Expected Instances");
        }
    }
}

// =============================================================================
// Enhancement 10: Compound predicates (ALIVE AND priority == "high")
// =============================================================================

#[tokio::test]
async fn find_alive_with_compound_filter() {
    let engine = setup_engine().await;
    spawn_ticket(&engine, "Active High", "high").await;
    spawn_ticket(&engine, "Active Low", "low").await;
    let id3 = spawn_ticket(&engine, "Closed High", "high").await;
    transition_to(&engine, &id3, "cancelled").await;

    let stmts =
        smql_parser::parse(r#"FIND Ticket WHERE ALIVE AND priority == "high""#).unwrap();
    if let Statement::Query(query) = &stmts[0] {
        let result = engine.execute_query(query).await.unwrap();
        if let smql_engine_core::query::QueryResult::Instances(insts) = result {
            assert_eq!(insts.len(), 1);
            assert_eq!(
                insts[0].data.get("title").unwrap(),
                &smql_ast::value::Value::Text("Active High".into())
            );
        } else {
            panic!("Expected Instances");
        }
    }
}

// =============================================================================
// Enhancement 10: Parser tests for predicates
// =============================================================================

#[test]
fn parse_alive_predicate() {
    let stmts = smql_parser::parse("FIND Ticket WHERE ALIVE").unwrap();
    if let Statement::Query(Query::Find(q)) = &stmts[0] {
        assert!(q.filter.is_some());
        let f = q.filter.as_ref().unwrap();
        assert!(matches!(
            f.kind,
            smql_ast::expression::ExpressionKind::Alive
        ));
    } else {
        panic!("Expected FIND");
    }
}

#[test]
fn parse_terminated_predicate() {
    let stmts = smql_parser::parse("FIND Ticket WHERE TERMINATED").unwrap();
    if let Statement::Query(Query::Find(q)) = &stmts[0] {
        let f = q.filter.as_ref().unwrap();
        assert!(matches!(
            f.kind,
            smql_ast::expression::ExpressionKind::Terminated
        ));
    } else {
        panic!("Expected FIND");
    }
}

#[test]
fn parse_stuck_in_predicate() {
    let stmts = smql_parser::parse(r#"FIND Ticket WHERE STUCK_IN("open", 24h)"#).unwrap();
    if let Statement::Query(Query::Find(q)) = &stmts[0] {
        let f = q.filter.as_ref().unwrap();
        match &f.kind {
            smql_ast::expression::ExpressionKind::StuckIn { state, duration } => {
                assert_eq!(state, "open");
                assert_eq!(duration.seconds, 86400); // 24h
            }
            _ => panic!("Expected StuckIn"),
        }
    } else {
        panic!("Expected FIND");
    }
}

#[test]
fn parse_has_visited_predicate() {
    let stmts = smql_parser::parse(r#"FIND Ticket WHERE HAS_VISITED("resolved")"#).unwrap();
    if let Statement::Query(Query::Find(q)) = &stmts[0] {
        let f = q.filter.as_ref().unwrap();
        assert!(matches!(
            &f.kind,
            smql_ast::expression::ExpressionKind::HasVisited(s) if s == "resolved"
        ));
    } else {
        panic!("Expected FIND");
    }
}

#[test]
fn parse_never_visited_predicate() {
    let stmts = smql_parser::parse(r#"FIND Ticket WHERE NEVER_VISITED("cancelled")"#).unwrap();
    if let Statement::Query(Query::Find(q)) = &stmts[0] {
        let f = q.filter.as_ref().unwrap();
        assert!(matches!(
            &f.kind,
            smql_ast::expression::ExpressionKind::NeverVisited(s) if s == "cancelled"
        ));
    } else {
        panic!("Expected FIND");
    }
}

// =============================================================================
// Enhancement 10: TRAIL SINCE/UNTIL
// =============================================================================

#[tokio::test]
async fn trail_since_filter() {
    let engine = setup_engine().await;
    let id = spawn_ticket(&engine, "Test", "high").await;
    transition_to(&engine, &id, "in_progress").await;
    transition_to(&engine, &id, "resolved").await;

    // Use a date far in the past — should include all entries
    let query_str = format!(
        r#"TRAIL OF "{}" WHERE SINCE "2020-01-01T00:00:00Z""#,
        id
    );
    let stmts = smql_parser::parse(&query_str).unwrap();
    if let Statement::Query(query) = &stmts[0] {
        let result = engine.execute_query(query).await.unwrap();
        if let smql_engine_core::query::QueryResult::Trail(entries) = result {
            assert!(entries.len() >= 2, "should include spawn + transitions");
        } else {
            panic!("Expected Trail");
        }
    }
}

#[tokio::test]
async fn trail_until_filter() {
    let engine = setup_engine().await;
    let id = spawn_ticket(&engine, "Test", "high").await;
    transition_to(&engine, &id, "in_progress").await;

    // Use a date far in the past — should exclude all entries
    let query_str = format!(
        r#"TRAIL OF "{}" WHERE UNTIL "2020-01-01T00:00:00Z""#,
        id
    );
    let stmts = smql_parser::parse(&query_str).unwrap();
    if let Statement::Query(query) = &stmts[0] {
        let result = engine.execute_query(query).await.unwrap();
        if let smql_engine_core::query::QueryResult::Trail(entries) = result {
            assert_eq!(entries.len(), 0, "all entries should be after 2020");
        } else {
            panic!("Expected Trail");
        }
    }
}
