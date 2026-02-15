#[cfg(test)]
mod catalog_tests {
    use crate::MachineCatalog;
    use smql_ast::machine::*;

    fn simple_machine(name: &str) -> MachineDefinition {
        let mut m = MachineDefinition::new(name.into(), "init".into());
        m.states = vec![
            StateDefinition::new("init".into()),
            StateDefinition::new("done".into()),
        ];
        m.terminal_states = vec!["done".into()];
        m.transitions = vec![TransitionDefinition::new(
            TransitionSource::State("init".into()),
            "done".into(),
        )];
        m
    }

    #[test]
    fn register_and_get() {
        let catalog = MachineCatalog::new();
        let m = simple_machine("TestMachine");
        catalog.register(m.clone()).unwrap();

        let retrieved = catalog.get("TestMachine").unwrap();
        assert_eq!(retrieved.name, "TestMachine");
        assert_eq!(retrieved.states.len(), 2);
    }

    #[test]
    fn get_nonexistent() {
        let catalog = MachineCatalog::new();
        let result = catalog.get("NonExistent");
        assert!(result.is_err());
    }

    #[test]
    fn contains() {
        let catalog = MachineCatalog::new();
        let m = simple_machine("Exists");
        catalog.register(m).unwrap();

        assert!(catalog.contains("Exists"));
        assert!(!catalog.contains("Missing"));
    }

    #[test]
    fn unregister() {
        let catalog = MachineCatalog::new();
        let m = simple_machine("ToRemove");
        catalog.register(m).unwrap();

        let removed = catalog.unregister("ToRemove").unwrap();
        assert_eq!(removed.name, "ToRemove");
        assert!(!catalog.contains("ToRemove"));
    }

    #[test]
    fn unregister_nonexistent() {
        let catalog = MachineCatalog::new();
        let result = catalog.unregister("Nope");
        assert!(result.is_err());
    }

    #[test]
    fn list_machines() {
        let catalog = MachineCatalog::new();
        catalog.register(simple_machine("A")).unwrap();
        catalog.register(simple_machine("B")).unwrap();

        let mut names = catalog.list();
        names.sort();
        assert_eq!(names, vec!["A", "B"]);
    }

    #[test]
    fn version() {
        let catalog = MachineCatalog::new();
        let m = simple_machine("Versioned");
        catalog.register(m.clone()).unwrap();
        assert_eq!(catalog.version("Versioned").unwrap(), 1);
    }

    #[test]
    fn update_increments_version() {
        let catalog = MachineCatalog::new();
        let m = simple_machine("Updatable");
        catalog.register(m.clone()).unwrap();

        let mut updated = m.clone();
        updated.states.push(StateDefinition::new("middle".into()));
        updated.transitions.push(TransitionDefinition::new(
            TransitionSource::State("init".into()),
            "middle".into(),
        ));
        catalog.update(updated).unwrap();

        assert_eq!(catalog.version("Updatable").unwrap(), 2);
        let def = catalog.get("Updatable").unwrap();
        assert_eq!(def.states.len(), 3);
    }

    #[test]
    fn serialize_deserialize() {
        let catalog = MachineCatalog::new();
        catalog.register(simple_machine("Machine1")).unwrap();
        catalog.register(simple_machine("Machine2")).unwrap();

        let json = catalog.serialize().unwrap();
        let restored = MachineCatalog::deserialize(&json).unwrap();

        assert!(restored.contains("Machine1"));
        assert!(restored.contains("Machine2"));
    }
}

#[cfg(test)]
mod validation_tests {
    use crate::MachineCatalog;
    use smql_ast::machine::*;
    use smql_ast::types::*;

    fn base_machine() -> MachineDefinition {
        let mut m = MachineDefinition::new("Test".into(), "open".into());
        m.states = vec![
            StateDefinition::new("open".into()),
            StateDefinition::new("closed".into()),
        ];
        m.terminal_states = vec!["closed".into()];
        m.transitions = vec![TransitionDefinition::new(
            TransitionSource::State("open".into()),
            "closed".into(),
        )];
        m
    }

    #[test]
    fn valid_machine_passes() {
        let catalog = MachineCatalog::new();
        let m = base_machine();
        let warnings = catalog.register(m).unwrap();
        // May have warnings but should not error
        assert!(warnings.is_empty());
    }

    #[test]
    fn invalid_initial_state() {
        let catalog = MachineCatalog::new();
        let mut m = base_machine();
        m.initial_state = "nonexistent".into();
        let result = catalog.register(m);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nonexistent"));
    }

    #[test]
    fn invalid_terminal_state() {
        let catalog = MachineCatalog::new();
        let mut m = base_machine();
        m.terminal_states = vec!["missing".into()];
        let result = catalog.register(m);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_transition_source() {
        let catalog = MachineCatalog::new();
        let mut m = base_machine();
        m.transitions.push(TransitionDefinition::new(
            TransitionSource::State("phantom".into()),
            "closed".into(),
        ));
        let result = catalog.register(m);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_transition_target() {
        let catalog = MachineCatalog::new();
        let mut m = base_machine();
        m.transitions.push(TransitionDefinition::new(
            TransitionSource::State("open".into()),
            "ghost".into(),
        ));
        let result = catalog.register(m);
        assert!(result.is_err());
    }

    #[test]
    fn warn_transition_from_terminal() {
        let catalog = MachineCatalog::new();
        let mut m = base_machine();
        m.transitions.push(TransitionDefinition::new(
            TransitionSource::State("closed".into()),
            "open".into(),
        ));
        let warnings = catalog.register(m).unwrap();
        assert!(warnings.iter().any(|w| w.message.contains("terminal state")));
    }

    #[test]
    fn warn_unreachable_state() {
        let catalog = MachineCatalog::new();
        let mut m = base_machine();
        m.states.push(StateDefinition::new("island".into()));
        let warnings = catalog.register(m).unwrap();
        assert!(warnings.iter().any(|w| w.message.contains("unreachable")));
    }

    #[test]
    fn warn_dead_end_state() {
        let catalog = MachineCatalog::new();
        let mut m = MachineDefinition::new("DeadEnd".into(), "a".into());
        m.states = vec![
            StateDefinition::new("a".into()),
            StateDefinition::new("b".into()),
            StateDefinition::new("c".into()),
        ];
        m.terminal_states = vec!["c".into()];
        m.transitions = vec![
            TransitionDefinition::new(TransitionSource::State("a".into()), "b".into()),
            // b has no outgoing and is not terminal
        ];
        let warnings = catalog.register(m).unwrap();
        assert!(warnings.iter().any(|w| w.message.contains("dead-end")));
    }

    #[test]
    fn wildcard_any_valid() {
        let catalog = MachineCatalog::new();
        let mut m = base_machine();
        m.states.push(StateDefinition::new("middle".into()));
        m.transitions.push(TransitionDefinition::new(
            TransitionSource::State("open".into()),
            "middle".into(),
        ));
        m.transitions.push(TransitionDefinition::new(
            TransitionSource::Any {
                except: vec!["closed".into()],
            },
            "closed".into(),
        ));
        let warnings = catalog.register(m).unwrap();
        // All states have outgoing via ANY, so no dead-end warnings for non-terminals
        assert!(!warnings.iter().any(|w| w.message.contains("dead-end")));
    }

    #[test]
    fn invalid_except_from_state() {
        let catalog = MachineCatalog::new();
        let mut m = base_machine();
        m.transitions.push(TransitionDefinition::new(
            TransitionSource::Any {
                except: vec!["phantom".into()],
            },
            "closed".into(),
        ));
        let result = catalog.register(m);
        assert!(result.is_err());
    }

    #[test]
    fn warn_ref_to_unknown_machine() {
        let catalog = MachineCatalog::new();
        let mut m = base_machine();
        m.data.push(DataFieldDefinition {
            name: "agent".into(),
            field_type: TypeDefinition::Ref("UnknownMachine".into()),
            constraints: vec![],
        });
        let warnings = catalog.register(m).unwrap();
        assert!(warnings.iter().any(|w| w.message.contains("UnknownMachine")));
    }

    #[test]
    fn warn_child_unknown_machine() {
        let catalog = MachineCatalog::new();
        let mut m = base_machine();
        m.children.push(smql_ast::machine::ChildDefinition {
            name: "items".into(),
            machine: "UnknownChild".into(),
            cardinality: smql_ast::machine::ChildCardinality::List {
                min: None,
                max: None,
            },
        });
        let warnings = catalog.register(m).unwrap();
        assert!(warnings.iter().any(|w| w.message.contains("UnknownChild")));
    }

    #[test]
    fn invalid_timeout_target() {
        let catalog = MachineCatalog::new();
        let mut m = base_machine();
        m.transitions[0].timeout = Some(smql_ast::machine::TimeoutClause {
            duration: smql_ast::value::SmqlDuration::from_hours(24),
            target_state: "nonexistent".into(),
        });
        let result = catalog.register(m);
        assert!(result.is_err());
    }
}
