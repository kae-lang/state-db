// SMQL Storage — Pluggable storage backends

pub mod instance;
pub mod memory;
#[cfg(feature = "rocksdb")]
pub mod rocksdb;
pub mod traits;

#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "rocksdb"))]
mod rocksdb_tests;

pub use instance::{
    Filter, FilterPredicate, Instance, InstanceId, Mutation, StoredTimer, TrailEntry, TrailFilter,
};
pub use memory::MemoryStorage;
#[cfg(feature = "rocksdb")]
pub use rocksdb::RocksDBStorage;
pub use traits::Storage;

#[cfg(test)]
mod coverage_tests {
    use super::instance::*;
    use super::memory::MemoryStorage;
    use super::traits::Storage;
    use chrono::{NaiveDate, Utc};
    use smql_ast::value::{SmqlDuration, Value};
    use std::collections::HashMap;

    // -----------------------------------------------------------------------
    // InstanceId tests
    // -----------------------------------------------------------------------

    #[test]
    fn instance_id_default_creates_new_id() {
        let id1 = InstanceId::default();
        let id2 = InstanceId::default();
        // Two default IDs should be distinct (ULIDs are unique)
        assert_ne!(id1, id2);
    }

    #[test]
    fn instance_id_display() {
        let id = InstanceId::new();
        let display = format!("{}", id);
        let as_str = id.as_str();
        assert_eq!(display, as_str);
        // ULID strings are 26 characters
        assert_eq!(display.len(), 26);
    }

    // -----------------------------------------------------------------------
    // FilterPredicate::matches() — Gt, Gte, Lt, Lte, IsNull, IsNotNull, And, Or
    // -----------------------------------------------------------------------

    fn data_with(key: &str, val: Value) -> HashMap<String, Value> {
        let mut data = HashMap::new();
        data.insert(key.to_string(), val);
        data
    }

    #[test]
    fn predicate_gt_true_and_false() {
        let data = data_with("x", Value::Int(10));
        assert!(FilterPredicate::Gt("x".to_string(), Value::Int(5)).matches(&data));
        assert!(!FilterPredicate::Gt("x".to_string(), Value::Int(10)).matches(&data));
        assert!(!FilterPredicate::Gt("x".to_string(), Value::Int(15)).matches(&data));
    }

    #[test]
    fn predicate_gte_true_and_false() {
        let data = data_with("x", Value::Int(10));
        assert!(FilterPredicate::Gte("x".to_string(), Value::Int(5)).matches(&data));
        assert!(FilterPredicate::Gte("x".to_string(), Value::Int(10)).matches(&data));
        assert!(!FilterPredicate::Gte("x".to_string(), Value::Int(15)).matches(&data));
    }

    #[test]
    fn predicate_lt_true_and_false() {
        let data = data_with("x", Value::Int(3));
        assert!(FilterPredicate::Lt("x".to_string(), Value::Int(5)).matches(&data));
        assert!(!FilterPredicate::Lt("x".to_string(), Value::Int(3)).matches(&data));
        assert!(!FilterPredicate::Lt("x".to_string(), Value::Int(1)).matches(&data));
    }

    #[test]
    fn predicate_lte_true_and_false() {
        let data = data_with("x", Value::Int(3));
        assert!(FilterPredicate::Lte("x".to_string(), Value::Int(5)).matches(&data));
        assert!(FilterPredicate::Lte("x".to_string(), Value::Int(3)).matches(&data));
        assert!(!FilterPredicate::Lte("x".to_string(), Value::Int(1)).matches(&data));
    }

    #[test]
    fn predicate_is_null_present_null() {
        let data = data_with("x", Value::Null);
        assert!(FilterPredicate::IsNull("x".to_string()).matches(&data));
    }

    #[test]
    fn predicate_is_null_missing_field() {
        let data: HashMap<String, Value> = HashMap::new();
        // Missing field is treated as null
        assert!(FilterPredicate::IsNull("x".to_string()).matches(&data));
    }

    #[test]
    fn predicate_is_null_present_non_null() {
        let data = data_with("x", Value::Int(1));
        assert!(!FilterPredicate::IsNull("x".to_string()).matches(&data));
    }

    #[test]
    fn predicate_is_not_null_present_value() {
        let data = data_with("x", Value::Text("hi".to_string()));
        assert!(FilterPredicate::IsNotNull("x".to_string()).matches(&data));
    }

    #[test]
    fn predicate_is_not_null_missing_field() {
        let data: HashMap<String, Value> = HashMap::new();
        assert!(!FilterPredicate::IsNotNull("x".to_string()).matches(&data));
    }

    #[test]
    fn predicate_is_not_null_present_null() {
        let data = data_with("x", Value::Null);
        assert!(!FilterPredicate::IsNotNull("x".to_string()).matches(&data));
    }

    #[test]
    fn predicate_and_both_true() {
        let mut data = HashMap::new();
        data.insert("a".to_string(), Value::Int(5));
        data.insert("b".to_string(), Value::Int(10));
        let pred = FilterPredicate::And(
            Box::new(FilterPredicate::Gt("a".to_string(), Value::Int(3))),
            Box::new(FilterPredicate::Lt("b".to_string(), Value::Int(20))),
        );
        assert!(pred.matches(&data));
    }

    #[test]
    fn predicate_and_one_false() {
        let mut data = HashMap::new();
        data.insert("a".to_string(), Value::Int(1));
        data.insert("b".to_string(), Value::Int(10));
        let pred = FilterPredicate::And(
            Box::new(FilterPredicate::Gt("a".to_string(), Value::Int(3))),
            Box::new(FilterPredicate::Lt("b".to_string(), Value::Int(20))),
        );
        assert!(!pred.matches(&data));
    }

    #[test]
    fn predicate_or_one_true() {
        let data = data_with("a", Value::Int(1));
        let pred = FilterPredicate::Or(
            Box::new(FilterPredicate::Gt("a".to_string(), Value::Int(3))),
            Box::new(FilterPredicate::Lt("a".to_string(), Value::Int(5))),
        );
        assert!(pred.matches(&data));
    }

    #[test]
    fn predicate_or_both_false() {
        let data = data_with("a", Value::Int(10));
        let pred = FilterPredicate::Or(
            Box::new(FilterPredicate::Lt("a".to_string(), Value::Int(3))),
            Box::new(FilterPredicate::Eq("a".to_string(), Value::Int(5))),
        );
        assert!(!pred.matches(&data));
    }

    // -----------------------------------------------------------------------
    // compare_values() — DateTime, Date, Duration, and incompatible types
    // -----------------------------------------------------------------------

    #[test]
    fn predicate_gt_datetime() {
        let dt1 = Utc::now();
        let dt2 = dt1 + chrono::Duration::hours(1);
        let data = data_with("ts", Value::DateTime(dt2));
        assert!(FilterPredicate::Gt("ts".to_string(), Value::DateTime(dt1)).matches(&data));
        assert!(!FilterPredicate::Gt("ts".to_string(), Value::DateTime(dt2)).matches(&data));
    }

    #[test]
    fn predicate_lt_date() {
        let d1 = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
        let data = data_with("d", Value::Date(d1));
        assert!(FilterPredicate::Lt("d".to_string(), Value::Date(d2)).matches(&data));
        assert!(!FilterPredicate::Lt("d".to_string(), Value::Date(d1)).matches(&data));
    }

    #[test]
    fn predicate_gte_duration() {
        let dur_short = SmqlDuration::from_seconds(60);
        let dur_long = SmqlDuration::from_seconds(120);
        let data = data_with("dur", Value::Duration(dur_long.clone()));
        assert!(FilterPredicate::Gte("dur".to_string(), Value::Duration(dur_short)).matches(&data));
        assert!(FilterPredicate::Gte("dur".to_string(), Value::Duration(dur_long)).matches(&data));
    }

    #[test]
    fn predicate_incompatible_types_returns_false() {
        // Int vs Text — compare_values returns None, so Gt returns false
        let data = data_with("x", Value::Int(10));
        assert!(
            !FilterPredicate::Gt("x".to_string(), Value::Text("hello".to_string())).matches(&data)
        );
        assert!(
            !FilterPredicate::Lt("x".to_string(), Value::Text("hello".to_string())).matches(&data)
        );
        assert!(
            !FilterPredicate::Gte("x".to_string(), Value::Text("hello".to_string())).matches(&data)
        );
        assert!(
            !FilterPredicate::Lte("x".to_string(), Value::Text("hello".to_string())).matches(&data)
        );
    }

    #[test]
    fn predicate_gt_missing_field_returns_false() {
        let data: HashMap<String, Value> = HashMap::new();
        assert!(!FilterPredicate::Gt("x".to_string(), Value::Int(5)).matches(&data));
    }

    // -----------------------------------------------------------------------
    // MemoryStorage mutation tests: RemoveField, IncrementField, AppendToList
    // -----------------------------------------------------------------------

    fn make_instance(machine: &str, state: &str) -> Instance {
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::Text("test".to_string()));
        Instance::new(machine.to_string(), state.to_string(), data)
    }

    #[tokio::test]
    async fn mutation_remove_field() {
        let storage = MemoryStorage::new();
        let mut inst = make_instance("Order", "open");
        inst.data
            .insert("extra".to_string(), Value::Text("val".to_string()));
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::RemoveField("extra".to_string())];
        storage.update_instance(&id, 1, &mutations).await.unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        assert!(!updated.data.contains_key("extra"));
        // "name" should still be there
        assert!(updated.data.contains_key("name"));
    }

    #[tokio::test]
    async fn mutation_increment_field() {
        let storage = MemoryStorage::new();
        let mut inst = make_instance("Order", "open");
        inst.data.insert("counter".to_string(), Value::Int(10));
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::IncrementField("counter".to_string(), 7)];
        storage.update_instance(&id, 1, &mutations).await.unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        assert_eq!(updated.data.get("counter"), Some(&Value::Int(17)));
    }

    #[tokio::test]
    async fn mutation_increment_field_negative_delta() {
        let storage = MemoryStorage::new();
        let mut inst = make_instance("Order", "open");
        inst.data.insert("counter".to_string(), Value::Int(10));
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::IncrementField("counter".to_string(), -3)];
        storage.update_instance(&id, 1, &mutations).await.unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        assert_eq!(updated.data.get("counter"), Some(&Value::Int(7)));
    }

    #[tokio::test]
    async fn mutation_append_to_list() {
        let storage = MemoryStorage::new();
        let mut inst = make_instance("Order", "open");
        inst.data.insert(
            "items".to_string(),
            Value::List(vec![Value::Text("a".to_string())]),
        );
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::AppendToList(
            "items".to_string(),
            Value::Text("b".to_string()),
        )];
        storage.update_instance(&id, 1, &mutations).await.unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        match updated.data.get("items") {
            Some(Value::List(items)) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], Value::Text("a".to_string()));
                assert_eq!(items[1], Value::Text("b".to_string()));
            }
            other => panic!("Expected List, got {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // delete_instance for non-existent ID (NotFound error)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn delete_nonexistent_instance_returns_not_found() {
        let storage = MemoryStorage::new();
        let id = InstanceId::new();
        let result = storage.delete_instance(&id).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_str = format!("{}", err);
        assert!(
            err_str.contains("not found") || err_str.contains("Not found"),
            "Expected 'not found' error, got: {}",
            err_str
        );
    }

    // -----------------------------------------------------------------------
    // MemoryStorage::default() equivalence to new()
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn memory_storage_default_is_equivalent_to_new() {
        let storage_default = MemoryStorage::default();
        let storage_new = MemoryStorage::new();

        // Both should start empty and work the same way
        let inst = make_instance("Test", "initial");
        let id = inst.id.clone();
        storage_default.store_instance(&inst).await.unwrap();

        let retrieved = storage_default.get_instance(&id).await.unwrap();
        assert!(retrieved.is_some());

        // new() storage should also be empty
        let result = storage_new.get_instance(&id).await.unwrap();
        assert!(result.is_none());

        // And should be able to store
        let inst2 = make_instance("Test", "initial");
        storage_new.store_instance(&inst2).await.unwrap();
        let retrieved2 = storage_new.get_instance(&inst2.id).await.unwrap();
        assert!(retrieved2.is_some());
    }
}
