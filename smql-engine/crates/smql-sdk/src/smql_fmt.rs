use crate::types::TransitionOptions;

/// Format a SPAWN command.
pub fn format_spawn(machine: &str, data: &serde_json::Value) -> String {
    let data_str = format_data_fields(data);
    format!("SPAWN {} {{{}}}", machine, data_str)
}

/// Format a TRANSITION command.
pub fn format_transition(
    machine: &str,
    instance_id: &str,
    to_state: &str,
    opts: &TransitionOptions,
) -> String {
    let mut s = format!("TRANSITION {} \"{}\" TO {}", machine, instance_id, to_state);
    if !opts.with_data.is_empty() {
        let fields: Vec<String> = opts
            .with_data
            .iter()
            .map(|(k, v)| format!("{}: {}", k, value_to_smql(v)))
            .collect();
        s.push_str(&format!(" WITH {{ {} }}", fields.join(", ")));
    }
    if let Some(ref memo) = opts.memo {
        s.push_str(&format!(" MEMO \"{}\"", escape_string(memo)));
    }
    if let Some(ref actor) = opts.as_actor {
        s.push_str(&format!(" AS \"{}\"", escape_string(actor)));
    }
    s
}

/// Format a TRY TRANSITION command.
pub fn format_try_transition(
    machine: &str,
    instance_id: &str,
    to_state: &str,
    opts: &TransitionOptions,
) -> String {
    let base = format_transition(machine, instance_id, to_state, opts);
    format!("TRY {}", base)
}

/// Format a FIND query.
pub fn format_find(
    machine: &str,
    filter: Option<&str>,
    sort: &[(String, String)],
    limit: Option<u64>,
    offset: Option<u64>,
) -> String {
    let mut s = format!("FIND {}", machine);
    if let Some(f) = filter {
        s.push_str(&format!(" WHERE {}", f));
    }
    if !sort.is_empty() {
        let clauses: Vec<String> = sort
            .iter()
            .map(|(field, dir)| format!("{} {}", field, dir))
            .collect();
        s.push_str(&format!(" SORT BY {}", clauses.join(", ")));
    }
    if let Some(n) = limit {
        s.push_str(&format!(" LIMIT {}", n));
    }
    if let Some(n) = offset {
        s.push_str(&format!(" OFFSET {}", n));
    }
    s
}

/// Format a GET query.
pub fn format_get(machine: &str, instance_id: &str) -> String {
    format!("GET {} \"{}\"", machine, instance_id)
}

/// Format a TRAIL query.
pub fn format_trail(instance_id: &str) -> String {
    format!("TRAIL OF \"{}\"", instance_id)
}

/// Format an AGGREGATE query.
pub fn format_aggregate(machine: &str, measure: Option<&str>, group_by: Option<&str>) -> String {
    let mut s = format!("AGGREGATE {}", machine);
    if let Some(m) = measure {
        s.push_str(&format!(" MEASURE {}", m));
    }
    if let Some(g) = group_by {
        s.push_str(&format!(" GROUP BY {}", g));
    }
    s
}

/// Convert a JSON value to an SMQL literal.
pub fn value_to_smql(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => format!("\"{}\"", escape_string(s)),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(value_to_smql).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(obj) => {
            let fields: Vec<String> = obj
                .iter()
                .map(|(k, v)| format!("{}: {}", k, value_to_smql(v)))
                .collect();
            format!("{{{}}}", fields.join(", "))
        }
    }
}

fn format_data_fields(data: &serde_json::Value) -> String {
    match data {
        serde_json::Value::Object(obj) if obj.is_empty() => String::new(),
        serde_json::Value::Object(obj) => {
            let fields: Vec<String> = obj
                .iter()
                .map(|(k, v)| format!("{}: {}", k, value_to_smql(v)))
                .collect();
            format!(" {} ", fields.join(", "))
        }
        _ => String::new(),
    }
}

fn escape_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_to_smql_primitives() {
        assert_eq!(value_to_smql(&serde_json::json!(null)), "NULL");
        assert_eq!(value_to_smql(&serde_json::json!(true)), "true");
        assert_eq!(value_to_smql(&serde_json::json!(false)), "false");
        assert_eq!(value_to_smql(&serde_json::json!(42)), "42");
        assert_eq!(value_to_smql(&serde_json::json!(3.125)), "3.125");
        assert_eq!(value_to_smql(&serde_json::json!("hello")), "\"hello\"");
    }

    #[test]
    fn test_value_to_smql_array() {
        assert_eq!(value_to_smql(&serde_json::json!([1, 2, 3])), "[1, 2, 3]");
    }

    #[test]
    fn test_value_to_smql_object() {
        let val = serde_json::json!({"name": "Alice"});
        let result = value_to_smql(&val);
        assert_eq!(result, "{name: \"Alice\"}");
    }

    #[test]
    fn test_format_spawn() {
        let data = serde_json::json!({"title": "Bug report"});
        let s = format_spawn("ticket", &data);
        assert!(s.starts_with("SPAWN ticket {"));
        assert!(s.contains("title: \"Bug report\""));
    }

    #[test]
    fn test_format_spawn_empty() {
        let s = format_spawn("counter", &serde_json::json!({}));
        assert_eq!(s, "SPAWN counter {}");
    }

    #[test]
    fn test_format_transition() {
        let opts = TransitionOptions::default();
        let s = format_transition("Machine", "abc-123", "active", &opts);
        assert_eq!(s, "TRANSITION Machine \"abc-123\" TO active");
    }

    #[test]
    fn test_format_transition_with_data_and_memo() {
        let opts = TransitionOptions {
            with_data: vec![("count".to_string(), serde_json::json!(10))],
            memo: Some("bump count".to_string()),
            as_actor: None,
            as_role: None,
            idempotency_key: None,
        };
        let s = format_transition("Machine", "id1", "next", &opts);
        assert!(s.contains("TRANSITION Machine"));
        assert!(s.contains("WITH { count: 10 }"));
        assert!(s.contains("MEMO \"bump count\""));
    }

    #[test]
    fn test_format_find() {
        let s = format_find(
            "ticket",
            Some("STATE IS open"),
            &[("created_at".to_string(), "DESC".to_string())],
            Some(10),
            Some(5),
        );
        assert_eq!(
            s,
            "FIND ticket WHERE STATE IS open SORT BY created_at DESC LIMIT 10 OFFSET 5"
        );
    }

    #[test]
    fn test_format_get() {
        assert_eq!(format_get("ticket", "abc"), "GET ticket \"abc\"");
    }

    #[test]
    fn test_format_trail() {
        assert_eq!(format_trail("abc"), "TRAIL OF \"abc\"");
    }

    #[test]
    fn test_format_aggregate() {
        let s = format_aggregate("ticket", Some("COUNT()"), Some("state"));
        assert_eq!(s, "AGGREGATE ticket MEASURE COUNT() GROUP BY state");
    }

    #[test]
    fn test_escape_string() {
        assert_eq!(escape_string("hello \"world\""), "hello \\\"world\\\"");
    }

    #[test]
    fn test_escape_string_with_backslash() {
        // Backslash should be escaped before quotes
        assert_eq!(escape_string(r"path\to\file"), r"path\\to\\file");
        assert_eq!(
            escape_string(r#"back\and"quote"#),
            r#"back\\and\"quote"#
        );
    }

    #[test]
    fn test_format_data_fields_non_object() {
        // Non-object values (array, string, number, etc.) should return empty string
        assert_eq!(format_data_fields(&serde_json::json!(42)), "");
        assert_eq!(format_data_fields(&serde_json::json!("hello")), "");
        assert_eq!(format_data_fields(&serde_json::json!([1, 2, 3])), "");
        assert_eq!(format_data_fields(&serde_json::json!(null)), "");
        assert_eq!(format_data_fields(&serde_json::json!(true)), "");
    }

    #[test]
    fn test_format_spawn_non_object_data() {
        // When data is not an object, format_data_fields returns empty string
        let s = format_spawn("counter", &serde_json::json!(42));
        assert_eq!(s, "SPAWN counter {}");
    }

    #[test]
    fn test_format_find_no_filter_no_sort() {
        let s = format_find("ticket", None, &[], None, None);
        assert_eq!(s, "FIND ticket");
    }

    #[test]
    fn test_format_find_with_offset_only() {
        let s = format_find("ticket", None, &[], None, Some(20));
        assert_eq!(s, "FIND ticket OFFSET 20");
    }

    #[test]
    fn test_format_find_multiple_sort_clauses() {
        let s = format_find(
            "ticket",
            None,
            &[
                ("priority".to_string(), "DESC".to_string()),
                ("created_at".to_string(), "ASC".to_string()),
            ],
            None,
            None,
        );
        assert_eq!(
            s,
            "FIND ticket SORT BY priority DESC, created_at ASC"
        );
    }

    #[test]
    fn test_format_aggregate_no_measure_no_group() {
        let s = format_aggregate("ticket", None, None);
        assert_eq!(s, "AGGREGATE ticket");
    }

    #[test]
    fn test_format_aggregate_measure_only() {
        let s = format_aggregate("ticket", Some("AVG(amount)"), None);
        assert_eq!(s, "AGGREGATE ticket MEASURE AVG(amount)");
    }

    #[test]
    fn test_format_aggregate_group_only() {
        let s = format_aggregate("ticket", None, Some("priority"));
        assert_eq!(s, "AGGREGATE ticket GROUP BY priority");
    }

    #[test]
    fn test_format_transition_with_actor() {
        let opts = TransitionOptions {
            with_data: vec![],
            memo: None,
            as_actor: Some("admin@example.com".to_string()),
            as_role: None,
            idempotency_key: None,
        };
        let s = format_transition("Machine", "id1", "next", &opts);
        assert_eq!(
            s,
            "TRANSITION Machine \"id1\" TO next AS \"admin@example.com\""
        );
    }

    #[test]
    fn test_format_transition_all_options() {
        let opts = TransitionOptions {
            with_data: vec![
                ("count".to_string(), serde_json::json!(10)),
                ("label".to_string(), serde_json::json!("test")),
            ],
            memo: Some("full options".to_string()),
            as_actor: Some("user1".to_string()),
            as_role: None,
            idempotency_key: None,
        };
        let s = format_transition("M", "id", "done", &opts);
        assert!(s.contains("WITH { count: 10, label: \"test\" }"));
        assert!(s.contains("MEMO \"full options\""));
        assert!(s.contains("AS \"user1\""));
    }

    #[test]
    fn test_format_try_transition() {
        let opts = TransitionOptions::default();
        let s = format_try_transition("Machine", "abc", "next", &opts);
        assert_eq!(s, "TRY TRANSITION Machine \"abc\" TO next");
    }

    #[test]
    fn test_format_try_transition_with_options() {
        let opts = TransitionOptions {
            with_data: vec![("x".to_string(), serde_json::json!(1))],
            memo: Some("try it".to_string()),
            as_actor: Some("bot".to_string()),
            as_role: None,
            idempotency_key: None,
        };
        let s = format_try_transition("M", "id", "done", &opts);
        assert!(s.starts_with("TRY TRANSITION M"));
        assert!(s.contains("WITH { x: 1 }"));
        assert!(s.contains("MEMO \"try it\""));
        assert!(s.contains("AS \"bot\""));
    }

    #[test]
    fn test_value_to_smql_nested_object() {
        let val = serde_json::json!({"outer": {"inner": 42}});
        let result = value_to_smql(&val);
        assert_eq!(result, "{outer: {inner: 42}}");
    }

    #[test]
    fn test_value_to_smql_empty_array() {
        assert_eq!(value_to_smql(&serde_json::json!([])), "[]");
    }

    #[test]
    fn test_value_to_smql_empty_object() {
        assert_eq!(value_to_smql(&serde_json::json!({})), "{}");
    }

    #[test]
    fn test_value_to_smql_string_with_quotes() {
        assert_eq!(
            value_to_smql(&serde_json::json!("say \"hello\"")),
            "\"say \\\"hello\\\"\""
        );
    }

    #[test]
    fn test_format_data_fields_with_data() {
        let data = serde_json::json!({"name": "Alice", "age": 30});
        let result = format_data_fields(&data);
        // format_data_fields wraps with spaces: " field1: v1, field2: v2 "
        assert!(result.starts_with(' '));
        assert!(result.ends_with(' '));
        assert!(result.contains("name: \"Alice\""));
        assert!(result.contains("age: 30"));
    }
}
