#[cfg(test)]
mod memory_storage_tests {
    use crate::instance::*;
    use crate::memory::MemoryStorage;
    use crate::traits::Storage;
    use chrono::Utc;
    use smql_ast::value::Value;
    use std::collections::HashMap;

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
        let storage = MemoryStorage::new();
        let inst = make_instance("Order", "open");

        storage.store_instance(&inst).await.unwrap();

        let retrieved = storage.get_instance(&inst.id).await.unwrap().unwrap();
        assert_eq!(retrieved.machine, "Order");
        assert_eq!(retrieved.state, "open");
        assert_eq!(retrieved.version, 1);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let storage = MemoryStorage::new();
        let id = InstanceId::new();
        let result = storage.get_instance(&id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn store_duplicate_fails() {
        let storage = MemoryStorage::new();
        let inst = make_instance("Order", "open");

        storage.store_instance(&inst).await.unwrap();
        let result = storage.store_instance(&inst).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn find_by_state() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn find_by_multiple_states() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn find_all_in_machine() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn find_with_predicate() {
        let storage = MemoryStorage::new();
        let mut inst1 = make_instance("Order", "open");
        inst1.data.insert("priority".to_string(), Value::Int(1));
        let mut inst2 = make_instance("Order", "open");
        inst2.data.insert("priority".to_string(), Value::Int(5));

        storage.store_instance(&inst1).await.unwrap();
        storage.store_instance(&inst2).await.unwrap();

        let filter = Filter {
            predicate: Some(FilterPredicate::Gt("priority".to_string(), Value::Int(3))),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].data.get("priority"), Some(&Value::Int(5)));
    }

    #[tokio::test]
    async fn find_with_limit_and_offset() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn update_instance_fields() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn update_version_conflict() {
        let storage = MemoryStorage::new();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::SetField("x".to_string(), Value::Int(1))];
        // Use wrong expected version
        let result = storage.update_instance(&id, 99, &mutations).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn transition_instance() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn transition_updates_state_index() {
        let storage = MemoryStorage::new();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let trail = make_trail_entry(&inst, "open", "closed", 1);
        storage
            .transition_instance(&id, 1, "closed", &[], trail)
            .await
            .unwrap();

        // Should not be in "open" state anymore
        let open_filter = Filter {
            state: Some("open".to_string()),
            ..Default::default()
        };
        let open = storage.find_instances("Order", &open_filter).await.unwrap();
        assert_eq!(open.len(), 0);

        // Should be in "closed" state
        let closed_filter = Filter {
            state: Some("closed".to_string()),
            ..Default::default()
        };
        let closed = storage
            .find_instances("Order", &closed_filter)
            .await
            .unwrap();
        assert_eq!(closed.len(), 1);
    }

    #[tokio::test]
    async fn transition_version_conflict() {
        let storage = MemoryStorage::new();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let trail = make_trail_entry(&inst, "open", "closed", 1);
        let result = storage
            .transition_instance(&id, 99, "closed", &[], trail)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn delete_instance() {
        let storage = MemoryStorage::new();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        storage.delete_instance(&id).await.unwrap();

        assert!(storage.get_instance(&id).await.unwrap().is_none());

        let filter = Filter::default();
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn delete_nonexistent_fails() {
        let storage = MemoryStorage::new();
        let id = InstanceId::new();
        let result = storage.delete_instance(&id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn count_by_state() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn trail_append_and_get() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn trail_via_transition() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn query_trails_by_state() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn query_trails_by_actor() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn query_trails_with_limit() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn increment_field_mutation() {
        let storage = MemoryStorage::new();
        let mut inst = make_instance("Order", "open");
        inst.data.insert("count".to_string(), Value::Int(10));
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::IncrementField("count".to_string(), 5)];
        storage.update_instance(&id, 1, &mutations).await.unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        assert_eq!(updated.data.get("count"), Some(&Value::Int(15)));
    }

    #[tokio::test]
    async fn append_to_list_mutation() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn remove_field_mutation() {
        let storage = MemoryStorage::new();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::RemoveField("name".to_string())];
        storage.update_instance(&id, 1, &mutations).await.unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        assert!(!updated.data.contains_key("name"));
    }

    #[tokio::test]
    async fn predicate_and_or() {
        let storage = MemoryStorage::new();

        let mut inst1 = make_instance("Order", "open");
        inst1.data.insert("priority".to_string(), Value::Int(1));
        inst1.data.insert("vip".to_string(), Value::Bool(false));

        let mut inst2 = make_instance("Order", "open");
        inst2.data.insert("priority".to_string(), Value::Int(5));
        inst2.data.insert("vip".to_string(), Value::Bool(true));

        let mut inst3 = make_instance("Order", "open");
        inst3.data.insert("priority".to_string(), Value::Int(3));
        inst3.data.insert("vip".to_string(), Value::Bool(true));

        storage.store_instance(&inst1).await.unwrap();
        storage.store_instance(&inst2).await.unwrap();
        storage.store_instance(&inst3).await.unwrap();

        // priority > 4 OR vip == true
        let filter = Filter {
            predicate: Some(FilterPredicate::Or(
                Box::new(FilterPredicate::Gt("priority".to_string(), Value::Int(4))),
                Box::new(FilterPredicate::Eq("vip".to_string(), Value::Bool(true))),
            )),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 2); // inst2 and inst3
    }

    #[tokio::test]
    async fn instance_id_roundtrip() {
        let id = InstanceId::new();
        let s = id.as_str();
        let id2 = InstanceId::from_string(&s).unwrap();
        assert_eq!(id, id2);
    }

    #[tokio::test]
    async fn filter_is_null() {
        let storage = MemoryStorage::new();

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
    }

    // --- Parent-child composition tests ---

    #[tokio::test]
    async fn store_child_with_parent_id() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn find_children_returns_correct_children() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn find_children_filters_by_machine() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn find_children_empty_when_no_children() {
        let storage = MemoryStorage::new();
        let parent = make_instance("Order", "open");
        let parent_id = parent.id.clone();
        storage.store_instance(&parent).await.unwrap();

        let children = storage.find_children(&parent_id, None).await.unwrap();
        assert!(children.is_empty());
    }

    #[tokio::test]
    async fn get_parent_returns_parent() {
        let storage = MemoryStorage::new();
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
    }

    #[tokio::test]
    async fn get_parent_returns_none_for_root() {
        let storage = MemoryStorage::new();
        let root = make_instance("Order", "open");
        let root_id = root.id.clone();
        storage.store_instance(&root).await.unwrap();

        let parent = storage.get_parent(&root_id).await.unwrap();
        assert!(parent.is_none());
    }

    #[tokio::test]
    async fn delete_child_removes_from_parent_index() {
        let storage = MemoryStorage::new();
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
            storage.find_children(&parent_id, None).await.unwrap().len(),
            1
        );

        storage.delete_instance(&child_id).await.unwrap();

        assert_eq!(
            storage.find_children(&parent_id, None).await.unwrap().len(),
            0
        );
    }

    #[tokio::test]
    async fn filter_eq() {
        let storage = MemoryStorage::new();
        let mut inst1 = make_instance("Order", "open");
        inst1.data.insert("priority".to_string(), Value::Int(3));
        let mut inst2 = make_instance("Order", "open");
        inst2.data.insert("priority".to_string(), Value::Int(5));

        storage.store_instance(&inst1).await.unwrap();
        storage.store_instance(&inst2).await.unwrap();

        let filter = Filter {
            predicate: Some(FilterPredicate::Eq("priority".to_string(), Value::Int(3))),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].data.get("priority"), Some(&Value::Int(3)));
    }

    #[tokio::test]
    async fn filter_ne() {
        let storage = MemoryStorage::new();
        let mut inst1 = make_instance("Order", "open");
        inst1.data.insert("priority".to_string(), Value::Int(3));
        let mut inst2 = make_instance("Order", "open");
        inst2.data.insert("priority".to_string(), Value::Int(5));

        storage.store_instance(&inst1).await.unwrap();
        storage.store_instance(&inst2).await.unwrap();

        let filter = Filter {
            predicate: Some(FilterPredicate::Ne("priority".to_string(), Value::Int(3))),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].data.get("priority"), Some(&Value::Int(5)));
    }

    #[tokio::test]
    async fn filter_gte() {
        let storage = MemoryStorage::new();
        for p in [1, 3, 5] {
            let mut inst = make_instance("Order", "open");
            inst.data.insert("priority".to_string(), Value::Int(p));
            storage.store_instance(&inst).await.unwrap();
        }

        let filter = Filter {
            predicate: Some(FilterPredicate::Gte("priority".to_string(), Value::Int(3))),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 2); // 3 and 5
    }

    #[tokio::test]
    async fn filter_lte() {
        let storage = MemoryStorage::new();
        for p in [1, 3, 5] {
            let mut inst = make_instance("Order", "open");
            inst.data.insert("priority".to_string(), Value::Int(p));
            storage.store_instance(&inst).await.unwrap();
        }

        let filter = Filter {
            predicate: Some(FilterPredicate::Lte("priority".to_string(), Value::Int(3))),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 2); // 1 and 3
    }

    #[tokio::test]
    async fn filter_lt() {
        let storage = MemoryStorage::new();
        for p in [1, 3, 5] {
            let mut inst = make_instance("Order", "open");
            inst.data.insert("priority".to_string(), Value::Int(p));
            storage.store_instance(&inst).await.unwrap();
        }

        let filter = Filter {
            predicate: Some(FilterPredicate::Lt("priority".to_string(), Value::Int(3))),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1); // only 1
        assert_eq!(results[0].data.get("priority"), Some(&Value::Int(1)));
    }

    #[tokio::test]
    async fn filter_is_not_null() {
        let storage = MemoryStorage::new();
        let mut inst1 = make_instance("Order", "open");
        inst1
            .data
            .insert("assignee".to_string(), Value::Text("alice".to_string()));
        let mut inst2 = make_instance("Order", "open");
        inst2.data.insert("assignee".to_string(), Value::Null);
        let inst3 = make_instance("Order", "open"); // no assignee field at all

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
    }

    #[tokio::test]
    async fn filter_and_composite() {
        let storage = MemoryStorage::new();
        for p in [1, 3, 5, 7, 9] {
            let mut inst = make_instance("Order", "open");
            inst.data.insert("priority".to_string(), Value::Int(p));
            storage.store_instance(&inst).await.unwrap();
        }

        // Range filter: priority > 2 AND priority < 8
        let filter = Filter {
            predicate: Some(FilterPredicate::And(
                Box::new(FilterPredicate::Gt("priority".to_string(), Value::Int(2))),
                Box::new(FilterPredicate::Lt("priority".to_string(), Value::Int(8))),
            )),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 3); // 3, 5, 7
    }

    #[tokio::test]
    async fn find_with_limit_and_offset_paging() {
        let storage = MemoryStorage::new();
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
    }

    // --- Cursor-based pagination tests ---

    #[tokio::test]
    async fn cursor_filters_by_after_id() {
        let storage = MemoryStorage::new();
        let mut ids = Vec::new();
        for _ in 0..5 {
            let inst = make_instance("Order", "open");
            ids.push(inst.id.as_str());
            storage.store_instance(&inst).await.unwrap();
        }
        ids.sort();

        // Use the 2nd ID as cursor — should get IDs 3, 4, 5
        let filter = Filter {
            after_id: Some(ids[1].clone()),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 3);
        for inst in &results {
            assert!(inst.id.as_str() > ids[1]);
        }
    }

    #[tokio::test]
    async fn cursor_with_limit() {
        let storage = MemoryStorage::new();
        let mut ids = Vec::new();
        for _ in 0..5 {
            let inst = make_instance("Order", "open");
            ids.push(inst.id.as_str());
            storage.store_instance(&inst).await.unwrap();
        }
        ids.sort();

        let filter = Filter {
            after_id: Some(ids[0].clone()),
            limit: Some(2),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].id.as_str() > ids[0]);
    }

    #[tokio::test]
    async fn cursor_at_end_returns_empty() {
        let storage = MemoryStorage::new();
        let mut ids = Vec::new();
        for _ in 0..3 {
            let inst = make_instance("Order", "open");
            ids.push(inst.id.as_str());
            storage.store_instance(&inst).await.unwrap();
        }
        ids.sort();

        // Use the last ID as cursor — should get nothing
        let filter = Filter {
            after_id: Some(ids[2].clone()),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn cursor_with_predicate() {
        let storage = MemoryStorage::new();
        let mut ids = Vec::new();
        for i in 0..5 {
            let mut inst = make_instance("Order", "open");
            inst.data.insert("priority".to_string(), Value::Int(i));
            ids.push(inst.id.as_str());
            storage.store_instance(&inst).await.unwrap();
        }
        ids.sort();

        // Cursor + predicate: after first ID, priority > 2
        let filter = Filter {
            after_id: Some(ids[0].clone()),
            predicate: Some(FilterPredicate::Gt("priority".to_string(), Value::Int(2))),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        // Should be instances with priority 3 and 4 whose IDs are > ids[0]
        for inst in &results {
            assert!(inst.id.as_str() > ids[0]);
            let p = match inst.data.get("priority") {
                Some(Value::Int(v)) => *v,
                _ => panic!("Expected Int priority"),
            };
            assert!(p > 2);
        }
    }

    // -----------------------------------------------------------------------
    // Timer persistence tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn timer_store_and_load() {
        let storage = MemoryStorage::new();
        let now = Utc::now();
        let timer = StoredTimer {
            instance_id: "inst_1".to_string(),
            machine: "Ticket".to_string(),
            from_state: "waiting".to_string(),
            target_state: "resolved".to_string(),
            deadline: now + chrono::Duration::hours(1),
            registered_at: now,
        };
        storage.store_timer(&timer).await.unwrap();

        let all = storage.load_all_timers().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].instance_id, "inst_1");
        assert_eq!(all[0].from_state, "waiting");
        assert_eq!(all[0].target_state, "resolved");
    }

    #[tokio::test]
    async fn timer_remove_specific() {
        let storage = MemoryStorage::new();
        let now = Utc::now();
        let timer = StoredTimer {
            instance_id: "inst_1".to_string(),
            machine: "Ticket".to_string(),
            from_state: "waiting".to_string(),
            target_state: "resolved".to_string(),
            deadline: now + chrono::Duration::hours(1),
            registered_at: now,
        };
        storage.store_timer(&timer).await.unwrap();
        storage.remove_timer("inst_1", "waiting").await.unwrap();

        let all = storage.load_all_timers().await.unwrap();
        assert!(all.is_empty());
    }

    #[tokio::test]
    async fn timer_remove_all_for_instance() {
        let storage = MemoryStorage::new();
        let now = Utc::now();
        for state in &["waiting", "open", "triaged"] {
            let timer = StoredTimer {
                instance_id: "inst_1".to_string(),
                machine: "Ticket".to_string(),
                from_state: state.to_string(),
                target_state: "resolved".to_string(),
                deadline: now + chrono::Duration::hours(1),
                registered_at: now,
            };
            storage.store_timer(&timer).await.unwrap();
        }
        // Also store for another instance
        let other = StoredTimer {
            instance_id: "inst_2".to_string(),
            machine: "Ticket".to_string(),
            from_state: "waiting".to_string(),
            target_state: "resolved".to_string(),
            deadline: now + chrono::Duration::hours(1),
            registered_at: now,
        };
        storage.store_timer(&other).await.unwrap();

        storage.remove_all_timers("inst_1").await.unwrap();

        let all = storage.load_all_timers().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].instance_id, "inst_2");
    }

    #[tokio::test]
    async fn timer_load_all_empty() {
        let storage = MemoryStorage::new();
        let all = storage.load_all_timers().await.unwrap();
        assert!(all.is_empty());
    }

    #[tokio::test]
    async fn timer_overwrite_existing() {
        let storage = MemoryStorage::new();
        let now = Utc::now();
        let timer1 = StoredTimer {
            instance_id: "inst_1".to_string(),
            machine: "Ticket".to_string(),
            from_state: "waiting".to_string(),
            target_state: "resolved".to_string(),
            deadline: now + chrono::Duration::hours(1),
            registered_at: now,
        };
        storage.store_timer(&timer1).await.unwrap();

        // Overwrite with different target
        let timer2 = StoredTimer {
            instance_id: "inst_1".to_string(),
            machine: "Ticket".to_string(),
            from_state: "waiting".to_string(),
            target_state: "closed".to_string(),
            deadline: now + chrono::Duration::hours(2),
            registered_at: now,
        };
        storage.store_timer(&timer2).await.unwrap();

        let all = storage.load_all_timers().await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].target_state, "closed");
    }

    #[tokio::test]
    async fn timer_remove_nonexistent_is_ok() {
        let storage = MemoryStorage::new();
        // Should not error
        storage
            .remove_timer("nonexistent", "whatever")
            .await
            .unwrap();
        storage.remove_all_timers("nonexistent").await.unwrap();
    }

    // --- Mutation type mismatch tests (silent no-op) ---

    #[tokio::test]
    async fn increment_on_non_int_is_noop() {
        let storage = MemoryStorage::new();
        let mut inst = make_instance("Order", "open");
        inst.data
            .insert("label".to_string(), Value::Text("hello".to_string()));
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::IncrementField("label".to_string(), 5)];
        storage.update_instance(&id, 1, &mutations).await.unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        assert_eq!(
            updated.data.get("label"),
            Some(&Value::Text("hello".to_string()))
        );
    }

    #[tokio::test]
    async fn increment_on_missing_field_is_noop() {
        let storage = MemoryStorage::new();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::IncrementField("nonexistent".to_string(), 1)];
        storage.update_instance(&id, 1, &mutations).await.unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        assert!(!updated.data.contains_key("nonexistent"));
    }

    #[tokio::test]
    async fn append_to_list_on_non_list_is_noop() {
        let storage = MemoryStorage::new();
        let mut inst = make_instance("Order", "open");
        inst.data.insert("count".to_string(), Value::Int(5));
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::AppendToList(
            "count".to_string(),
            Value::Text("item".to_string()),
        )];
        storage.update_instance(&id, 1, &mutations).await.unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        assert_eq!(updated.data.get("count"), Some(&Value::Int(5)));
    }

    #[tokio::test]
    async fn append_to_list_on_missing_field_is_noop() {
        let storage = MemoryStorage::new();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let mutations = vec![Mutation::AppendToList(
            "no_such_list".to_string(),
            Value::Int(1),
        )];
        storage.update_instance(&id, 1, &mutations).await.unwrap();

        let updated = storage.get_instance(&id).await.unwrap().unwrap();
        assert!(!updated.data.contains_key("no_such_list"));
    }

    // --- Bulk update tests ---

    #[tokio::test]
    async fn bulk_update_instances() {
        let storage = MemoryStorage::new();
        for _ in 0..3 {
            let mut inst = make_instance("Order", "open");
            inst.data.insert("score".to_string(), Value::Int(0));
            storage.store_instance(&inst).await.unwrap();
        }
        // Also add a different machine
        let other = make_instance("Ticket", "open");
        storage.store_instance(&other).await.unwrap();

        let mutations = vec![Mutation::SetField(
            "score".to_string(),
            Value::Int(100),
        )];
        let count = storage.bulk_update_instances("Order", &mutations).await.unwrap();
        assert_eq!(count, 3);

        let filter = Filter::default();
        let results = storage.find_instances("Order", &filter).await.unwrap();
        for r in &results {
            assert_eq!(r.data.get("score"), Some(&Value::Int(100)));
        }

        // Ticket should not be updated
        let ticket = storage.get_instance(&other.id).await.unwrap().unwrap();
        assert!(!ticket.data.contains_key("score"));
    }

    // --- Migrate instances state tests ---

    #[tokio::test]
    async fn migrate_instances_state() {
        let storage = MemoryStorage::new();
        for _ in 0..3 {
            storage
                .store_instance(&make_instance("Order", "old_state"))
                .await
                .unwrap();
        }
        storage
            .store_instance(&make_instance("Order", "other"))
            .await
            .unwrap();

        let count = storage
            .migrate_instances_state("Order", "old_state", "new_state")
            .await
            .unwrap();
        assert_eq!(count, 3);

        let filter = Filter {
            state: Some("new_state".to_string()),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 3);

        let old_filter = Filter {
            state: Some("old_state".to_string()),
            ..Default::default()
        };
        let old_results = storage.find_instances("Order", &old_filter).await.unwrap();
        assert!(old_results.is_empty());
    }

    // --- Offset beyond results clears ---

    #[tokio::test]
    async fn find_with_offset_beyond_results() {
        let storage = MemoryStorage::new();
        storage
            .store_instance(&make_instance("Order", "open"))
            .await
            .unwrap();

        let filter = Filter {
            offset: Some(100),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert!(results.is_empty());
    }

    // --- Get parent of nonexistent child ---

    #[tokio::test]
    async fn get_parent_nonexistent_child_returns_none() {
        let storage = MemoryStorage::new();
        let fake_id = InstanceId::new();
        let parent = storage.get_parent(&fake_id).await.unwrap();
        assert!(parent.is_none());
    }

    // --- InstanceId tests ---

    #[test]
    fn instance_id_from_string_invalid() {
        let result = InstanceId::from_string("invalid-ulid");
        assert!(result.is_err());
    }

    #[test]
    fn instance_id_display() {
        let id = InstanceId::new();
        let s = format!("{}", id);
        assert_eq!(s, id.as_str());
    }

    #[test]
    fn instance_id_default() {
        let id = InstanceId::default();
        assert!(!id.as_str().is_empty());
    }

    // --- Memory storage default ---

    #[tokio::test]
    async fn memory_storage_default() {
        let storage = MemoryStorage::default();
        let filter = Filter::default();
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert!(results.is_empty());
    }

    // --- compare_values edge cases ---

    #[test]
    fn compare_values_text() {
        use crate::instance::FilterPredicate;
        let data: HashMap<String, Value> = vec![
            ("name".to_string(), Value::Text("banana".into())),
        ]
        .into_iter()
        .collect();

        assert!(FilterPredicate::Gt("name".to_string(), Value::Text("apple".into())).matches(&data));
        assert!(FilterPredicate::Lt("name".to_string(), Value::Text("cherry".into())).matches(&data));
    }

    #[test]
    fn compare_values_incompatible_types() {
        use crate::instance::FilterPredicate;
        let data: HashMap<String, Value> = vec![
            ("x".to_string(), Value::Int(5)),
        ]
        .into_iter()
        .collect();

        // Int vs Text comparison should return None from compare_values,
        // so Gt returns false
        assert!(!FilterPredicate::Gt("x".to_string(), Value::Text("hello".into())).matches(&data));
    }

    #[test]
    fn compare_values_float() {
        use crate::instance::FilterPredicate;
        let data: HashMap<String, Value> = vec![
            ("x".to_string(), Value::Float(3.14)),
        ]
        .into_iter()
        .collect();

        assert!(FilterPredicate::Gt("x".to_string(), Value::Float(2.0)).matches(&data));
        assert!(FilterPredicate::Lt("x".to_string(), Value::Float(4.0)).matches(&data));
    }

    #[test]
    fn compare_values_datetime() {
        use crate::instance::FilterPredicate;
        let now = Utc::now();
        let earlier = now - chrono::Duration::hours(1);
        let data: HashMap<String, Value> = vec![
            ("ts".to_string(), Value::DateTime(now)),
        ]
        .into_iter()
        .collect();

        assert!(FilterPredicate::Gt("ts".to_string(), Value::DateTime(earlier)).matches(&data));
    }

    #[test]
    fn compare_values_date() {
        use crate::instance::FilterPredicate;
        use chrono::NaiveDate;
        let d1 = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2025, 6, 14).unwrap();
        let data: HashMap<String, Value> = vec![
            ("d".to_string(), Value::Date(d1)),
        ]
        .into_iter()
        .collect();

        assert!(FilterPredicate::Gt("d".to_string(), Value::Date(d2)).matches(&data));
    }

    #[test]
    fn compare_values_duration() {
        use crate::instance::FilterPredicate;
        use smql_ast::value::SmqlDuration;
        let data: HashMap<String, Value> = vec![
            ("dur".to_string(), Value::Duration(SmqlDuration::from_hours(2))),
        ]
        .into_iter()
        .collect();

        assert!(FilterPredicate::Gt(
            "dur".to_string(),
            Value::Duration(SmqlDuration::from_hours(1))
        )
        .matches(&data));
    }

    // --- Ne with missing field ---

    #[test]
    fn ne_with_missing_field_returns_true() {
        use crate::instance::FilterPredicate;
        let data: HashMap<String, Value> = HashMap::new();
        // Ne should return true when field doesn't exist (None != val)
        assert!(FilterPredicate::Ne("missing".to_string(), Value::Int(1)).matches(&data));
    }

    // --- query_trails with from_state and time filters ---

    #[tokio::test]
    async fn query_trails_by_from_state() {
        let storage = MemoryStorage::new();
        let inst = make_instance("Order", "open");
        storage.store_instance(&inst).await.unwrap();

        let entry1 = make_trail_entry(&inst, "open", "processing", 1);
        let entry2 = make_trail_entry(&inst, "processing", "closed", 2);
        storage.append_trail_entry(&entry1).await.unwrap();
        storage.append_trail_entry(&entry2).await.unwrap();

        let filter = TrailFilter {
            from_state: Some("processing".to_string()),
            ..Default::default()
        };
        let results = storage.query_trails("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].from_state, "processing");
    }

    #[tokio::test]
    async fn query_trails_empty_machine() {
        let storage = MemoryStorage::new();
        let filter = TrailFilter::default();
        let results = storage.query_trails("NonExistent", &filter).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn count_by_state_empty_machine() {
        let storage = MemoryStorage::new();
        let counts = storage.count_by_state("NonExistent").await.unwrap();
        assert!(counts.is_empty());
    }

    #[tokio::test]
    async fn delete_instance_cleans_up_timers() {
        let storage = MemoryStorage::new();
        let inst = make_instance("Order", "open");
        let id = inst.id.clone();
        storage.store_instance(&inst).await.unwrap();

        let now = Utc::now();
        let timer = StoredTimer {
            instance_id: id.as_str(),
            machine: "Order".to_string(),
            from_state: "open".to_string(),
            target_state: "closed".to_string(),
            deadline: now + chrono::Duration::hours(1),
            registered_at: now,
        };
        storage.store_timer(&timer).await.unwrap();
        assert_eq!(storage.load_all_timers().await.unwrap().len(), 1);

        storage.delete_instance(&id).await.unwrap();
        assert_eq!(storage.load_all_timers().await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn update_nonexistent_instance_fails() {
        let storage = MemoryStorage::new();
        let id = InstanceId::new();
        let mutations = vec![Mutation::SetField("x".to_string(), Value::Int(1))];
        let result = storage.update_instance(&id, 1, &mutations).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn transition_nonexistent_instance_fails() {
        let storage = MemoryStorage::new();
        let id = InstanceId::new();
        let trail = TrailEntry {
            instance_id: id.clone(),
            machine: "Order".to_string(),
            sequence: 1,
            from_state: "open".to_string(),
            to_state: "closed".to_string(),
            transition_name: None,
            actor: None,
            memo: None,
            timestamp: Utc::now(),
            data_snapshot: None,
        };
        let result = storage
            .transition_instance(&id, 1, "closed", &[], trail)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn get_trail_nonexistent_instance() {
        let storage = MemoryStorage::new();
        let id = InstanceId::new();
        let result = storage.get_trail(&id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn append_trail_entry_creates_trail_if_not_stored() {
        let storage = MemoryStorage::new();
        let id = InstanceId::new();
        let entry = TrailEntry {
            instance_id: id.clone(),
            machine: "Order".to_string(),
            sequence: 0,
            from_state: "".to_string(),
            to_state: "open".to_string(),
            transition_name: None,
            actor: None,
            memo: None,
            timestamp: Utc::now(),
            data_snapshot: None,
        };
        storage.append_trail_entry(&entry).await.unwrap();
        let trail = storage.get_trail(&id).await.unwrap();
        assert_eq!(trail.len(), 1);
    }

    // --- query_trails with time-based filters ---

    #[tokio::test]
    async fn query_trails_with_after_filter() {
        let storage = MemoryStorage::new();
        let inst = make_instance("Order", "open");
        storage.store_instance(&inst).await.unwrap();

        let now = Utc::now();
        let past = now - chrono::Duration::hours(2);
        let mid = now - chrono::Duration::hours(1);

        let mut entry1 = make_trail_entry(&inst, "open", "processing", 1);
        entry1.timestamp = past;
        let mut entry2 = make_trail_entry(&inst, "processing", "closed", 2);
        entry2.timestamp = now;

        storage.append_trail_entry(&entry1).await.unwrap();
        storage.append_trail_entry(&entry2).await.unwrap();

        let filter = TrailFilter {
            after: Some(mid),
            ..Default::default()
        };
        let results = storage.query_trails("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].to_state, "closed");
    }

    #[tokio::test]
    async fn query_trails_with_before_filter() {
        let storage = MemoryStorage::new();
        let inst = make_instance("Order", "open");
        storage.store_instance(&inst).await.unwrap();

        let now = Utc::now();
        let past = now - chrono::Duration::hours(2);
        let mid = now - chrono::Duration::hours(1);

        let mut entry1 = make_trail_entry(&inst, "open", "processing", 1);
        entry1.timestamp = past;
        let mut entry2 = make_trail_entry(&inst, "processing", "closed", 2);
        entry2.timestamp = now;

        storage.append_trail_entry(&entry1).await.unwrap();
        storage.append_trail_entry(&entry2).await.unwrap();

        let filter = TrailFilter {
            before: Some(mid),
            ..Default::default()
        };
        let results = storage.query_trails("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].to_state, "processing");
    }

    #[tokio::test]
    async fn query_trails_with_after_and_before() {
        let storage = MemoryStorage::new();
        let inst = make_instance("Order", "open");
        storage.store_instance(&inst).await.unwrap();

        let now = Utc::now();
        let t1 = now - chrono::Duration::hours(3);
        let t2 = now - chrono::Duration::hours(2);
        let t3 = now - chrono::Duration::hours(1);

        let mut entry1 = make_trail_entry(&inst, "open", "processing", 1);
        entry1.timestamp = t1;
        let mut entry2 = make_trail_entry(&inst, "processing", "review", 2);
        entry2.timestamp = t2;
        let mut entry3 = make_trail_entry(&inst, "review", "closed", 3);
        entry3.timestamp = now;

        storage.append_trail_entry(&entry1).await.unwrap();
        storage.append_trail_entry(&entry2).await.unwrap();
        storage.append_trail_entry(&entry3).await.unwrap();

        // Filter: after t1 and before t3 -- should only get entry2
        let filter = TrailFilter {
            after: Some(t1 + chrono::Duration::seconds(1)),
            before: Some(t3),
            ..Default::default()
        };
        let results = storage.query_trails("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].to_state, "review");
    }

    #[tokio::test]
    async fn find_with_offset_within_range() {
        let storage = MemoryStorage::new();
        for _ in 0..5 {
            storage
                .store_instance(&make_instance("Order", "open"))
                .await
                .unwrap();
        }

        // Offset 3, no limit: should get 2 results
        let filter = Filter {
            offset: Some(3),
            ..Default::default()
        };
        let results = storage.find_instances("Order", &filter).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    // --- Instance new_child test ---

    #[test]
    fn instance_new_child_sets_parent() {
        let parent_id = InstanceId::new();
        let child = Instance::new_child(
            "Item".to_string(),
            "pending".to_string(),
            HashMap::new(),
            parent_id.clone(),
            "Order".to_string(),
        );
        assert_eq!(child.parent_id.as_ref().unwrap(), &parent_id);
        assert_eq!(child.parent_machine.as_ref().unwrap(), "Order");
        assert_eq!(child.state, "pending");
        assert_eq!(child.machine, "Item");
        assert_eq!(child.version, 1);
        assert_eq!(child.trail_length, 0);
    }
}
