#[cfg(all(test, feature = "rocksdb"))]
mod rocksdb_storage_tests {
    use crate::instance::*;
    use crate::rocksdb::RocksDBStorage;
    use crate::traits::Storage;
    use chrono::Utc;
    use smql_ast::value::Value;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_path() -> String {
        let c = COUNTER.fetch_add(1, Ordering::SeqCst);
        format!(
            "/tmp/smql-test-rocksdb-{}-{}",
            std::process::id(),
            c
        )
    }

    fn open_storage() -> (RocksDBStorage, String) {
        let path = temp_path();
        let storage = RocksDBStorage::open(&path).unwrap();
        (storage, path)
    }

    fn cleanup(path: &str) {
        let _ = RocksDBStorage::destroy(path);
    }

    fn make_instance(machine: &str, state: &str) -> Instance {
        let mut data = HashMap::new();
        data.insert("name".to_string(), Value::Text("test".to_string()));
        Instance::new(machine.to_string(), state.to_string(), data)
    }

    fn make_trail_entry(instance: &Instance, from: &str, to: &str, seq: u64) -> TrailEntry {
        TrailEntry {
            instance_id: instance.id.clone(),
            machine: instance.machine.clone(),
            sequence: seq,
            from_state: from.to_string(),
            to_state: to.to_string(),
            transition_name: None,
            actor: Some("test_user".to_string()),
            memo: None,
            timestamp: Utc::now(),
            data_snapshot: None,
        }
    }

    #[tokio::test]
    async fn store_and_get_instance() {
        let (storage, path) = open_storage();
        let inst = make_instance("Order", "open");

        storage.store_instance(&inst).await.unwrap();

        let retrieved = storage.get_instance(&inst.id).await.unwrap().unwrap();
        assert_eq!(retrieved.machine, "Order");
        assert_eq!(retrieved.state, "open");
        assert_eq!(retrieved.version, 1);

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let (storage, path) = open_storage();
        let id = InstanceId::new();
        let result = storage.get_instance(&id).await.unwrap();
        assert!(result.is_none());

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn store_duplicate_fails() {
        let (storage, path) = open_storage();
        let inst = make_instance("Order", "open");

        storage.store_instance(&inst).await.unwrap();
        let result = storage.store_instance(&inst).await;
        assert!(result.is_err());

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn find_by_state() {
        let (storage, path) = open_storage();
        let inst1 = make_instance("Order", "open");
        let inst2 = make_instance("Order", "closed");
        let inst3 = make_instance("Order", "open");

        storage.store_instance(&inst1).await.unwrap();
        storage.store_instance(&inst2).await.unwrap();
        storage.store_instance(&inst3).await.unwrap();

        let filter = Filter {
            state: Some("open".to_string()),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.state == "open"));

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn find_by_multiple_states() {
        let (storage, path) = open_storage();
        let inst1 = make_instance("Order", "open");
        let inst2 = make_instance("Order", "processing");
        let inst3 = make_instance("Order", "closed");

        storage.store_instance(&inst1).await.unwrap();
        storage.store_instance(&inst2).await.unwrap();
        storage.store_instance(&inst3).await.unwrap();

        let filter = Filter {
            states: Some(vec!["open".to_string(), "processing".to_string()]),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 2);

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn find_all_in_machine() {
        let (storage, path) = open_storage();
        storage
            .store_instance(&make_instance("Order", "open"))
            .await
            .unwrap();
        storage
            .store_instance(&make_instance("Order", "closed"))
            .await
            .unwrap();
        storage
            .store_instance(&make_instance("Ticket", "open"))
            .await
            .unwrap();

        let filter = Filter::default();
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 2);

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn find_with_predicate() {
        let (storage, path) = open_storage();
        let mut inst1 = make_instance("Order", "open");
        inst1.data.insert("priority".to_string(), Value::Int(1));
        let mut inst2 = make_instance("Order", "open");
        inst2.data.insert("priority".to_string(), Value::Int(5));

        storage.store_instance(&inst1).await.unwrap();
        storage.store_instance(&inst2).await.unwrap();

        let filter = Filter {
            predicate: Some(FilterPredicate::Gt(
                "priority".to_string(),
                Value::Int(3),
            )),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].data.get("priority"), Some(&Value::Int(5)));

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn find_with_limit_and_offset() {
        let (storage, path) = open_storage();
        for i in 0..10 {
            let mut inst = make_instance("Order", "open");
            inst.data.insert("index".to_string(), Value::Int(i));
            storage.store_instance(&inst).await.unwrap();
        }

        let filter = Filter {
            limit: Some(3),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 3);

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn update_instance_fields() {
        let (storage, path) = open_storage();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![
            Mutation::SetField("name".to_string(), Value::Text("updated".to_string())),
            Mutation::SetField("new_field".to_string(), Value::Int(42)),
        ];
        storage.update_instance(&id, 1, &mutations).await.unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        assert_eq!(
            updated.data.get("name"),
            Some(&Value::Text("updated".to_string()))
        );
        assert_eq!(updated.data.get("new_field"), Some(&Value::Int(42)));
        assert_eq!(updated.version, 2);

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn update_version_conflict() {
        let (storage, path) = open_storage();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::SetField("x".to_string(), Value::Int(1))];
        let result = storage.update_instance(&id, 99, &mutations).await;
        assert!(result.is_err());

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn transition_instance() {
        let (storage, path) = open_storage();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let trail = make_trail_entry(&inst, "open", "processing", 1);
        let mutations = vec![Mutation::SetField(
            "assigned".to_string(),
            Value::Bool(true),
        )];

        storage
            .transition_instance(&id, 1, "processing", &mutations, trail)
            .await
            .unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        assert_eq!(updated.state, "processing");
        assert_eq!(updated.version, 2);
        assert_eq!(updated.trail_length, 1);
        assert_eq!(updated.data.get("assigned"), Some(&Value::Bool(true)));

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn transition_updates_state_index() {
        let (storage, path) = open_storage();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let trail = make_trail_entry(&inst, "open", "closed", 1);
        storage
            .transition_instance(&id, 1, "closed", &[], trail)
            .await
            .unwrap();

        let open_filter = Filter {
            state: Some("open".to_string()),
            ..Default::default()
        };
        let open = storage.find_instances("Order", &open_filter).await.unwrap();
        assert_eq!(open.len(), 0);

        let closed_filter = Filter {
            state: Some("closed".to_string()),
            ..Default::default()
        };
        let closed = storage
            .find_instances("Order", &closed_filter)
            .await
            .unwrap();
        assert_eq!(closed.len(), 1);

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn transition_version_conflict() {
        let (storage, path) = open_storage();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let trail = make_trail_entry(&inst, "open", "closed", 1);
        let result = storage
            .transition_instance(&id, 99, "closed", &[], trail)
            .await;
        assert!(result.is_err());

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn delete_instance() {
        let (storage, path) = open_storage();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        storage.delete_instance(&id).await.unwrap();

        assert!(storage.get_instance(&id).await.unwrap().is_none());

        let filter = Filter::default();
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 0);

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn delete_nonexistent_fails() {
        let (storage, path) = open_storage();
        let id = InstanceId::new();
        let result = storage.delete_instance(&id).await;
        assert!(result.is_err());

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn count_by_state() {
        let (storage, path) = open_storage();
        storage
            .store_instance(&make_instance("Order", "open"))
            .await
            .unwrap();
        storage
            .store_instance(&make_instance("Order", "open"))
            .await
            .unwrap();
        storage
            .store_instance(&make_instance("Order", "closed"))
            .await
            .unwrap();
        storage
            .store_instance(&make_instance("Ticket", "open"))
            .await
            .unwrap();

        let counts = storage.count_by_state("Order").await.unwrap();
        assert_eq!(counts.get("open"), Some(&2));
        assert_eq!(counts.get("closed"), Some(&1));

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn trail_append_and_get() {
        let (storage, path) = open_storage();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let entry1 = make_trail_entry(&inst, "open", "processing", 1);
        let entry2 = make_trail_entry(&inst, "processing", "closed", 2);

        storage.append_trail_entry(&entry1).await.unwrap();
        storage.append_trail_entry(&entry2).await.unwrap();

        let trail = storage.get_trail(&id).await.unwrap();
        assert_eq!(trail.len(), 2);
        assert_eq!(trail[0].from_state, "open");
        assert_eq!(trail[1].from_state, "processing");

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn trail_via_transition() {
        let (storage, path) = open_storage();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let trail = make_trail_entry(&inst, "open", "closed", 1);
        storage
            .transition_instance(&id, 1, "closed", &[], trail)
            .await
            .unwrap();

        let entries = storage.get_trail(&id).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].from_state, "open");
        assert_eq!(entries[0].to_state, "closed");

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn query_trails_by_state() {
        let (storage, path) = open_storage();
        let inst = make_instance("Order", "open");
        storage.store_instance(&inst).await.unwrap();

        let entry1 = make_trail_entry(&inst, "open", "processing", 1);
        let entry2 = make_trail_entry(&inst, "processing", "closed", 2);
        storage.append_trail_entry(&entry1).await.unwrap();
        storage.append_trail_entry(&entry2).await.unwrap();

        let filter = TrailFilter {
            to_state: Some("closed".to_string()),
            ..Default::default()
        };
        let results = storage.query_trails("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].to_state, "closed");

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn query_trails_by_actor() {
        let (storage, path) = open_storage();
        let inst = make_instance("Order", "open");
        storage.store_instance(&inst).await.unwrap();

        let mut entry1 = make_trail_entry(&inst, "open", "processing", 1);
        entry1.actor = Some("alice".to_string());
        let mut entry2 = make_trail_entry(&inst, "processing", "closed", 2);
        entry2.actor = Some("bob".to_string());

        storage.append_trail_entry(&entry1).await.unwrap();
        storage.append_trail_entry(&entry2).await.unwrap();

        let filter = TrailFilter {
            actor: Some("alice".to_string()),
            ..Default::default()
        };
        let results = storage.query_trails("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].actor, Some("alice".to_string()));

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn query_trails_with_limit() {
        let (storage, path) = open_storage();
        let inst = make_instance("Order", "open");
        storage.store_instance(&inst).await.unwrap();

        for i in 0..5 {
            let entry = TrailEntry {
                instance_id: inst.id.clone(),
                machine: "Order".to_string(),
                sequence: i,
                from_state: format!("s{}", i),
                to_state: format!("s{}", i + 1),
                transition_name: None,
                actor: None,
                memo: None,
                timestamp: Utc::now(),
                data_snapshot: None,
            };
            storage.append_trail_entry(&entry).await.unwrap();
        }

        let filter = TrailFilter {
            limit: Some(3),
            ..Default::default()
        };
        let results = storage.query_trails("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 3);

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn increment_field_mutation() {
        let (storage, path) = open_storage();
        let mut inst = make_instance("Order", "open");
        inst.data.insert("count".to_string(), Value::Int(10));
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::IncrementField("count".to_string(), 5)];
        storage.update_instance(&id, 1, &mutations).await.unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        assert_eq!(updated.data.get("count"), Some(&Value::Int(15)));

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn append_to_list_mutation() {
        let (storage, path) = open_storage();
        let mut inst = make_instance("Order", "open");
        inst.data.insert(
            "tags".to_string(),
            Value::List(vec![Value::Text("a".to_string())]),
        );
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::AppendToList(
            "tags".to_string(),
            Value::Text("b".to_string()),
        )];
        storage.update_instance(&id, 1, &mutations).await.unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        if let Some(Value::List(tags)) = updated.data.get("tags") {
            assert_eq!(tags.len(), 2);
            assert_eq!(tags[1], Value::Text("b".to_string()));
        } else {
            panic!("Expected List value for tags");
        }

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn remove_field_mutation() {
        let (storage, path) = open_storage();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::RemoveField("name".to_string())];
        storage.update_instance(&id, 1, &mutations).await.unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        assert!(!updated.data.contains_key("name"));

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn predicate_and_or() {
        let (storage, path) = open_storage();

        let mut inst1 = make_instance("Order", "open");
        inst1
            .data
            .insert("priority".to_string(), Value::Int(1));
        inst1.data.insert("vip".to_string(), Value::Bool(false));

        let mut inst2 = make_instance("Order", "open");
        inst2
            .data
            .insert("priority".to_string(), Value::Int(5));
        inst2.data.insert("vip".to_string(), Value::Bool(true));

        let mut inst3 = make_instance("Order", "open");
        inst3
            .data
            .insert("priority".to_string(), Value::Int(3));
        inst3.data.insert("vip".to_string(), Value::Bool(true));

        storage.store_instance(&inst1).await.unwrap();
        storage.store_instance(&inst2).await.unwrap();
        storage.store_instance(&inst3).await.unwrap();

        // priority > 4 OR vip == true
        let filter = Filter {
            predicate: Some(FilterPredicate::Or(
                Box::new(FilterPredicate::Gt(
                    "priority".to_string(),
                    Value::Int(4),
                )),
                Box::new(FilterPredicate::Eq(
                    "vip".to_string(),
                    Value::Bool(true),
                )),
            )),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 2); // inst2 and inst3

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn filter_is_null() {
        let (storage, path) = open_storage();

        let mut inst1 = make_instance("Order", "open");
        inst1.data.insert("notes".to_string(), Value::Null);

        let mut inst2 = make_instance("Order", "open");
        inst2
            .data
            .insert("notes".to_string(), Value::Text("hello".to_string()));

        storage.store_instance(&inst1).await.unwrap();
        storage.store_instance(&inst2).await.unwrap();

        let filter = Filter {
            predicate: Some(FilterPredicate::IsNull("notes".to_string())),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);

        drop(storage);
        cleanup(&path);
    }

    // --- Parent-child composition tests ---

    #[tokio::test]
    async fn store_child_with_parent_id() {
        let (storage, path) = open_storage();
        let parent = make_instance("Order", "open");
        let parent_id = parent.id.clone();
        storage.store_instance(&parent).await.unwrap();

        let child = Instance::new_child(
            "LineItem".to_string(),
            "pending".to_string(),
            HashMap::new(),
            parent_id.clone(),
            "Order".to_string(),
        );
        storage.store_instance(&child).await.unwrap();

        let retrieved = storage.get_instance(&child.id).await.unwrap().unwrap();
        assert_eq!(retrieved.parent_id.as_ref().unwrap(), &parent_id);
        assert_eq!(retrieved.parent_machine.as_ref().unwrap(), "Order");

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn find_children_returns_correct_children() {
        let (storage, path) = open_storage();
        let parent = make_instance("Order", "open");
        let parent_id = parent.id.clone();
        storage.store_instance(&parent).await.unwrap();

        let child1 = Instance::new_child(
            "LineItem".to_string(),
            "pending".to_string(),
            HashMap::new(),
            parent_id.clone(),
            "Order".to_string(),
        );
        let child2 = Instance::new_child(
            "LineItem".to_string(),
            "pending".to_string(),
            HashMap::new(),
            parent_id.clone(),
            "Order".to_string(),
        );
        storage.store_instance(&child1).await.unwrap();
        storage.store_instance(&child2).await.unwrap();

        let children = storage.find_children(&parent_id, None).await.unwrap();
        assert_eq!(children.len(), 2);

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn find_children_filters_by_machine() {
        let (storage, path) = open_storage();
        let parent = make_instance("Order", "open");
        let parent_id = parent.id.clone();
        storage.store_instance(&parent).await.unwrap();

        let child1 = Instance::new_child(
            "LineItem".to_string(),
            "pending".to_string(),
            HashMap::new(),
            parent_id.clone(),
            "Order".to_string(),
        );
        let child2 = Instance::new_child(
            "Shipment".to_string(),
            "pending".to_string(),
            HashMap::new(),
            parent_id.clone(),
            "Order".to_string(),
        );
        storage.store_instance(&child1).await.unwrap();
        storage.store_instance(&child2).await.unwrap();

        let line_items = storage
            .find_children(&parent_id, Some("LineItem"))
            .await
            .unwrap();
        assert_eq!(line_items.len(), 1);
        assert_eq!(line_items[0].machine, "LineItem");

        let shipments = storage
            .find_children(&parent_id, Some("Shipment"))
            .await
            .unwrap();
        assert_eq!(shipments.len(), 1);
        assert_eq!(shipments[0].machine, "Shipment");

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn find_children_empty_when_no_children() {
        let (storage, path) = open_storage();
        let parent = make_instance("Order", "open");
        let parent_id = parent.id.clone();
        storage.store_instance(&parent).await.unwrap();

        let children = storage.find_children(&parent_id, None).await.unwrap();
        assert!(children.is_empty());

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn get_parent_returns_parent() {
        let (storage, path) = open_storage();
        let parent = make_instance("Order", "open");
        let parent_id = parent.id.clone();
        storage.store_instance(&parent).await.unwrap();

        let child = Instance::new_child(
            "LineItem".to_string(),
            "pending".to_string(),
            HashMap::new(),
            parent_id,
            "Order".to_string(),
        );
        let child_id = child.id.clone();
        storage.store_instance(&child).await.unwrap();

        let found_parent = storage.get_parent(&child_id).await.unwrap().unwrap();
        assert_eq!(found_parent.machine, "Order");

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn get_parent_returns_none_for_root() {
        let (storage, path) = open_storage();
        let root = make_instance("Order", "open");
        let root_id = root.id.clone();
        storage.store_instance(&root).await.unwrap();

        let parent = storage.get_parent(&root_id).await.unwrap();
        assert!(parent.is_none());

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn delete_child_removes_from_parent_index() {
        let (storage, path) = open_storage();
        let parent = make_instance("Order", "open");
        let parent_id = parent.id.clone();
        storage.store_instance(&parent).await.unwrap();

        let child = Instance::new_child(
            "LineItem".to_string(),
            "pending".to_string(),
            HashMap::new(),
            parent_id.clone(),
            "Order".to_string(),
        );
        let child_id = child.id.clone();
        storage.store_instance(&child).await.unwrap();

        assert_eq!(
            storage
                .find_children(&parent_id, None)
                .await
                .unwrap()
                .len(),
            1
        );

        storage.delete_instance(&child_id).await.unwrap();

        assert_eq!(
            storage
                .find_children(&parent_id, None)
                .await
                .unwrap()
                .len(),
            0
        );

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn filter_eq() {
        let (storage, path) = open_storage();
        let mut inst1 = make_instance("Order", "open");
        inst1
            .data
            .insert("priority".to_string(), Value::Int(3));
        let mut inst2 = make_instance("Order", "open");
        inst2
            .data
            .insert("priority".to_string(), Value::Int(5));

        storage.store_instance(&inst1).await.unwrap();
        storage.store_instance(&inst2).await.unwrap();

        let filter = Filter {
            predicate: Some(FilterPredicate::Eq(
                "priority".to_string(),
                Value::Int(3),
            )),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].data.get("priority"), Some(&Value::Int(3)));

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn filter_ne() {
        let (storage, path) = open_storage();
        let mut inst1 = make_instance("Order", "open");
        inst1
            .data
            .insert("priority".to_string(), Value::Int(3));
        let mut inst2 = make_instance("Order", "open");
        inst2
            .data
            .insert("priority".to_string(), Value::Int(5));

        storage.store_instance(&inst1).await.unwrap();
        storage.store_instance(&inst2).await.unwrap();

        let filter = Filter {
            predicate: Some(FilterPredicate::Ne(
                "priority".to_string(),
                Value::Int(3),
            )),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].data.get("priority"), Some(&Value::Int(5)));

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn filter_gte() {
        let (storage, path) = open_storage();
        for p in [1, 3, 5] {
            let mut inst = make_instance("Order", "open");
            inst.data.insert("priority".to_string(), Value::Int(p));
            storage.store_instance(&inst).await.unwrap();
        }

        let filter = Filter {
            predicate: Some(FilterPredicate::Gte(
                "priority".to_string(),
                Value::Int(3),
            )),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 2); // 3 and 5

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn filter_lte() {
        let (storage, path) = open_storage();
        for p in [1, 3, 5] {
            let mut inst = make_instance("Order", "open");
            inst.data.insert("priority".to_string(), Value::Int(p));
            storage.store_instance(&inst).await.unwrap();
        }

        let filter = Filter {
            predicate: Some(FilterPredicate::Lte(
                "priority".to_string(),
                Value::Int(3),
            )),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 2); // 1 and 3

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn filter_lt() {
        let (storage, path) = open_storage();
        for p in [1, 3, 5] {
            let mut inst = make_instance("Order", "open");
            inst.data.insert("priority".to_string(), Value::Int(p));
            storage.store_instance(&inst).await.unwrap();
        }

        let filter = Filter {
            predicate: Some(FilterPredicate::Lt(
                "priority".to_string(),
                Value::Int(3),
            )),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].data.get("priority"), Some(&Value::Int(1)));

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn filter_is_not_null() {
        let (storage, path) = open_storage();
        let mut inst1 = make_instance("Order", "open");
        inst1
            .data
            .insert("assignee".to_string(), Value::Text("alice".to_string()));
        let mut inst2 = make_instance("Order", "open");
        inst2.data.insert("assignee".to_string(), Value::Null);
        let inst3 = make_instance("Order", "open");

        storage.store_instance(&inst1).await.unwrap();
        storage.store_instance(&inst2).await.unwrap();
        storage.store_instance(&inst3).await.unwrap();

        let filter = Filter {
            predicate: Some(FilterPredicate::IsNotNull("assignee".to_string())),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].data.get("assignee"),
            Some(&Value::Text("alice".to_string()))
        );

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn filter_and_composite() {
        let (storage, path) = open_storage();
        for p in [1, 3, 5, 7, 9] {
            let mut inst = make_instance("Order", "open");
            inst.data.insert("priority".to_string(), Value::Int(p));
            storage.store_instance(&inst).await.unwrap();
        }

        // Range filter: priority > 2 AND priority < 8
        let filter = Filter {
            predicate: Some(FilterPredicate::And(
                Box::new(FilterPredicate::Gt(
                    "priority".to_string(),
                    Value::Int(2),
                )),
                Box::new(FilterPredicate::Lt(
                    "priority".to_string(),
                    Value::Int(8),
                )),
            )),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 3); // 3, 5, 7

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn find_with_limit_and_offset_paging() {
        let (storage, path) = open_storage();
        for i in 0..5 {
            let mut inst = make_instance("Order", "open");
            inst.data.insert("index".to_string(), Value::Int(i));
            storage.store_instance(&inst).await.unwrap();
        }

        let filter = Filter {
            limit: Some(2),
            offset: Some(2),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 2);

        drop(storage);
        cleanup(&path);
    }

    // --- Schema migration tests ---

    #[tokio::test]
    async fn migrate_instances_state() {
        let (storage, path) = open_storage();
        storage
            .store_instance(&make_instance("Order", "open"))
            .await
            .unwrap();
        storage
            .store_instance(&make_instance("Order", "open"))
            .await
            .unwrap();
        storage
            .store_instance(&make_instance("Order", "closed"))
            .await
            .unwrap();

        let count = storage
            .migrate_instances_state("Order", "open", "pending")
            .await
            .unwrap();
        assert_eq!(count, 2);

        // Verify state index updated
        let open_filter = Filter {
            state: Some("open".to_string()),
            ..Default::default()
        };
        let open = storage.find_instances("Order", &open_filter).await.unwrap();
        assert_eq!(open.len(), 0);

        let pending_filter = Filter {
            state: Some("pending".to_string()),
            ..Default::default()
        };
        let pending = storage
            .find_instances("Order", &pending_filter)
            .await
            .unwrap();
        assert_eq!(pending.len(), 2);

        // closed instances unaffected
        let closed_filter = Filter {
            state: Some("closed".to_string()),
            ..Default::default()
        };
        let closed = storage
            .find_instances("Order", &closed_filter)
            .await
            .unwrap();
        assert_eq!(closed.len(), 1);

        drop(storage);
        cleanup(&path);
    }

    #[tokio::test]
    async fn bulk_update_instances() {
        let (storage, path) = open_storage();
        let mut inst1 = make_instance("Order", "open");
        inst1.data.insert("count".to_string(), Value::Int(10));
        let mut inst2 = make_instance("Order", "open");
        inst2.data.insert("count".to_string(), Value::Int(20));
        let inst3 = make_instance("Ticket", "open"); // different machine

        storage.store_instance(&inst1).await.unwrap();
        storage.store_instance(&inst2).await.unwrap();
        storage.store_instance(&inst3).await.unwrap();

        let mutations = vec![Mutation::SetField(
            "migrated".to_string(),
            Value::Bool(true),
        )];
        let count = storage
            .bulk_update_instances("Order", &mutations)
            .await
            .unwrap();
        assert_eq!(count, 2);

        // Verify updates applied
        let updated1 = storage.get_instance(&inst1.id).await.unwrap().unwrap();
        assert_eq!(updated1.data.get("migrated"), Some(&Value::Bool(true)));
        assert_eq!(updated1.version, 2);

        let updated2 = storage.get_instance(&inst2.id).await.unwrap().unwrap();
        assert_eq!(updated2.data.get("migrated"), Some(&Value::Bool(true)));

        // Ticket unaffected
        let ticket = storage.get_instance(&inst3.id).await.unwrap().unwrap();
        assert!(!ticket.data.contains_key("migrated"));
        assert_eq!(ticket.version, 1);

        drop(storage);
        cleanup(&path);
    }

    // --- Persistence test ---

    #[tokio::test]
    async fn data_persists_across_reopen() {
        let path = temp_path();

        let inst_id;
        {
            let storage = RocksDBStorage::open(&path).unwrap();
            let inst = make_instance("Order", "open");
            inst_id = inst.id.clone();
            storage.store_instance(&inst).await.unwrap();
            // storage dropped here, DB closed
        }

        {
            let storage = RocksDBStorage::open(&path).unwrap();
            let retrieved = storage.get_instance(&inst_id).await.unwrap().unwrap();
            assert_eq!(retrieved.machine, "Order");
            assert_eq!(retrieved.state, "open");
        }

        cleanup(&path);
    }
}
