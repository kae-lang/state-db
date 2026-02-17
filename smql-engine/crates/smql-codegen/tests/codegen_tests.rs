use smql_ast::types::TypeDefinition;
use smql_codegen::rust_gen::{to_pascal_case, to_snake_case};
use smql_codegen::type_map::{smql_type_to_rust, smql_type_to_rust_with_enum};
use smql_codegen::{CodeGenerator, CodegenError};

fn support_ticket_smql() -> &'static str {
    r#"DEFINE MACHINE SupportTicket (
        DATA {
            subject : TEXT
            description : TEXT
            priority : ENUM(low, medium, high, critical)
            assignee : TEXT -> OPTIONAL
            resolution_note : TEXT -> OPTIONAL
        }
        STATES { open, assigned, in_progress, resolved, closed, reopened }
        INITIAL STATE open
        TERMINAL STATES { closed }
        TRANSITIONS {
            open -> assigned {}
            assigned -> in_progress {}
            in_progress -> resolved {}
            resolved -> closed {}
            resolved -> reopened {}
            reopened -> in_progress {}
        }
    )"#
}

fn order_smql() -> &'static str {
    r#"
    DEFINE MACHINE Order (
        DATA {
            total : INT
            currency : TEXT
            items : LIST(TEXT)
        }
        STATES { cart, submitted, paid, shipped, delivered, cancelled }
        INITIAL STATE cart
        TERMINAL STATES { delivered, cancelled }
        TRANSITIONS {
            cart -> submitted {}
            submitted -> paid {}
            paid -> shipped {}
            shipped -> delivered {}
            cart -> cancelled {}
            submitted -> cancelled {}
        }
    )
    DEFINE MACHINE OrderItem (
        DATA {
            name : TEXT
            quantity : INT
        }
        STATES { active, removed }
        INITIAL STATE active
        TERMINAL STATES { removed }
        TRANSITIONS {
            active -> removed {}
        }
    )
    DEFINE MACHINE Payment (
        DATA {
            amount : INT
            method : TEXT
        }
        STATES { pending, completed, failed }
        INITIAL STATE pending
        TERMINAL STATES { completed, failed }
        TRANSITIONS {
            pending -> completed {}
            pending -> failed {}
        }
    )"#
}

// 1. State enum generation (correct variants, is_terminal)
#[test]
fn test_state_enum_generation() {
    let gen = CodeGenerator::from_source(support_ticket_smql()).unwrap();
    let output = gen.generate_combined_rust();

    assert!(output.contains("pub enum State {"));
    assert!(output.contains("Open,"));
    assert!(output.contains("Assigned,"));
    assert!(output.contains("InProgress,"));
    assert!(output.contains("Resolved,"));
    assert!(output.contains("Closed,"));
    assert!(output.contains("Reopened,"));

    // is_terminal should only match Closed
    assert!(output.contains("Self::Closed"));
    assert!(output.contains("fn is_terminal"));
}

// 2. Data struct generation (field names, types, Option wrapping)
#[test]
fn test_data_struct_generation() {
    let gen = CodeGenerator::from_source(support_ticket_smql()).unwrap();
    let output = gen.generate_combined_rust();

    assert!(output.contains("pub struct SupportTicketData {"));
    assert!(output.contains("pub subject: String,"));
    assert!(output.contains("pub description: String,"));
    // Optional fields should be Option<...>
    assert!(output.contains("pub assignee: Option<String>,"));
    assert!(output.contains("pub resolution_note: Option<String>,"));
}

// 3. Enum field generation (ENUM(low,medium,high))
#[test]
fn test_enum_field_generation() {
    let gen = CodeGenerator::from_source(support_ticket_smql()).unwrap();
    let output = gen.generate_combined_rust();

    assert!(output.contains("pub enum SupportTicketPriority {"));
    assert!(output.contains("Low,"));
    assert!(output.contains("Medium,"));
    assert!(output.contains("High,"));
    assert!(output.contains("Critical,"));

    // Data struct should use the enum type
    assert!(output.contains("pub priority: SupportTicketPriority,"));
}

// 4. Machine impl generation (MACHINE_NAME, trait impl)
#[test]
fn test_machine_impl() {
    let gen = CodeGenerator::from_source(support_ticket_smql()).unwrap();
    let output = gen.generate_combined_rust();

    assert!(output.contains("pub struct SupportTicket;"));
    assert!(output.contains(r#"pub const MACHINE_NAME: &'static str = "SupportTicket";"#));
}

// 5. Type mapping (each SMQL type)
#[test]
fn test_type_mapping() {
    assert_eq!(smql_type_to_rust(&TypeDefinition::Text), "String");
    assert_eq!(smql_type_to_rust(&TypeDefinition::Int), "i64");
    assert_eq!(smql_type_to_rust(&TypeDefinition::Float), "f64");
    assert_eq!(smql_type_to_rust(&TypeDefinition::Bool), "bool");
    assert_eq!(smql_type_to_rust(&TypeDefinition::Uuid), "String");
    assert_eq!(smql_type_to_rust(&TypeDefinition::Date), "String");
    assert_eq!(smql_type_to_rust(&TypeDefinition::DateTime), "String");
    assert_eq!(smql_type_to_rust(&TypeDefinition::Duration), "String");
    assert_eq!(
        smql_type_to_rust(&TypeDefinition::Ref("Agent".to_string())),
        "String"
    );
    assert_eq!(
        smql_type_to_rust(&TypeDefinition::List(Box::new(TypeDefinition::Text))),
        "Vec<String>"
    );
    assert_eq!(
        smql_type_to_rust(&TypeDefinition::Set(Box::new(TypeDefinition::Int))),
        "Vec<i64>"
    );
    assert_eq!(
        smql_type_to_rust(&TypeDefinition::Money("USD".to_string())),
        "(i64, String)"
    );
    assert_eq!(
        smql_type_to_rust(&TypeDefinition::Json),
        "serde_json::Value"
    );
    assert_eq!(smql_type_to_rust(&TypeDefinition::Blob), "Vec<u8>");
    assert_eq!(
        smql_type_to_rust(&TypeDefinition::Map(
            Box::new(TypeDefinition::Text),
            Box::new(TypeDefinition::Int)
        )),
        "std::collections::BTreeMap<String, i64>"
    );
}

// 6. Multi-machine file (order.smql → 3 modules)
#[test]
fn test_multi_machine() {
    let gen = CodeGenerator::from_source(order_smql()).unwrap();
    assert_eq!(gen.machines().len(), 3);

    let files = gen.generate_rust();
    assert_eq!(files.len(), 3);
    assert_eq!(files[0].path, "order.rs");
    assert_eq!(files[1].path, "order_item.rs");
    assert_eq!(files[2].path, "payment.rs");

    // Combined output should have all 3 modules
    let combined = gen.generate_combined_rust();
    assert!(combined.contains("pub mod order {"));
    assert!(combined.contains("pub mod order_item {"));
    assert!(combined.contains("pub mod payment {"));
}

// 7. State name conversion (snake_case → PascalCase)
#[test]
fn test_name_conversions() {
    assert_eq!(to_pascal_case("in_progress"), "InProgress");
    assert_eq!(to_pascal_case("open"), "Open");
    assert_eq!(to_pascal_case("SupportTicket"), "SupportTicket"); // Already PascalCase
    assert_eq!(to_snake_case("SupportTicket"), "support_ticket");
    assert_eq!(to_snake_case("OrderItem"), "order_item");
    assert_eq!(to_snake_case("counter"), "counter");
}

// 8. Full roundtrip: parse support_ticket.smql → generate → verify output
#[test]
fn test_roundtrip_support_ticket() {
    let gen = CodeGenerator::from_source(support_ticket_smql()).unwrap();
    let output = gen.generate_combined_rust();

    // Module structure
    assert!(output.contains("pub mod support_ticket {"));

    // State enum has all states
    assert!(output.contains("pub enum State {"));
    assert!(output.contains("fn from_name("));
    assert!(output.contains("fn as_str("));
    assert!(output.contains("fn is_terminal("));

    // Data struct
    assert!(output.contains("pub struct SupportTicketData {"));

    // Machine marker
    assert!(output.contains("pub struct SupportTicket;"));
}

// 9. Full roundtrip: parse order.smql → generate → verify output
#[test]
fn test_roundtrip_order() {
    let gen = CodeGenerator::from_source(order_smql()).unwrap();
    let output = gen.generate_combined_rust();

    // Order module
    assert!(output.contains("pub mod order {"));
    assert!(output.contains("pub struct OrderData {"));
    assert!(output.contains("pub total: i64,"));
    assert!(output.contains("pub items: Vec<String>,"));
    assert!(output.contains("Cart,"));
    assert!(output.contains("Delivered,"));
    assert!(output.contains("Cancelled,"));
    assert!(output.contains("Self::Delivered | Self::Cancelled"));

    // OrderItem module
    assert!(output.contains("pub mod order_item {"));
    assert!(output.contains("pub struct OrderItemData {"));

    // Payment module
    assert!(output.contains("pub mod payment {"));
    assert!(output.contains("Self::Completed | Self::Failed"));
}

// 10. Edge cases (empty data, no transitions, errors)
#[test]
fn test_empty_data() {
    let smql = r#"DEFINE MACHINE counter (
        STATES { idle, done }
        INITIAL STATE idle
        TERMINAL STATES { done }
        TRANSITIONS { idle -> done {} }
    )"#;

    let gen = CodeGenerator::from_source(smql).unwrap();
    let output = gen.generate_combined_rust();

    // Should still generate a data struct (empty)
    assert!(output.contains("pub struct CounterData {"));
    assert!(output.contains("pub enum State {"));
    assert!(output.contains("Idle,"));
    assert!(output.contains("Done,"));
}

#[test]
fn test_no_machines_error() {
    // An empty input or non-machine statement should fail
    let result = CodeGenerator::from_source("");
    assert!(matches!(
        result,
        Err(CodegenError::Parse(_)) | Err(CodegenError::NoMachines)
    ));
}

// ============================================================
// Coverage gap tests: generator.rs — from_files()
// ============================================================

// 11. from_files() — IO error when file does not exist
#[test]
fn test_from_files_missing_file() {
    let result = CodeGenerator::from_files(&["/nonexistent/path/to/machine.smql"]);
    assert!(result.is_err());
    match result {
        Err(CodegenError::Io(_)) => {} // expected
        Err(other) => panic!("Expected Io error, got: {:?}", other),
        Ok(_) => panic!("Expected error, got Ok"),
    }
}

// 12. from_files() — parse error in a valid file with invalid SMQL content
#[test]
fn test_from_files_parse_error() {
    use std::io::Write;
    let dir = std::env::temp_dir().join("smql_codegen_test_parse_error");
    let _ = std::fs::create_dir_all(&dir);
    let file_path = dir.join("bad.smql");
    let mut f = std::fs::File::create(&file_path).unwrap();
    write!(f, "THIS IS NOT VALID SMQL AT ALL {{{{").unwrap();
    drop(f);

    let path_str = file_path.to_str().unwrap();
    let result = CodeGenerator::from_files(&[path_str]);
    assert!(result.is_err());
    match result {
        Err(CodegenError::Parse(msg)) => {
            // Verify error message contains the file path
            assert!(
                msg.contains("bad.smql"),
                "Parse error should include file path, got: {}",
                msg
            );
        }
        Err(other) => panic!("Expected Parse error, got: {:?}", other),
        Ok(_) => panic!("Expected error, got Ok"),
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// 13. from_files() — file is readable but contains no machine definitions
#[test]
fn test_from_files_no_machines() {
    use std::io::Write;
    let dir = std::env::temp_dir().join("smql_codegen_test_no_machines");
    let _ = std::fs::create_dir_all(&dir);
    let file_path = dir.join("empty.smql");
    let mut f = std::fs::File::create(&file_path).unwrap();
    // A valid but empty-ish file that the parser returns 0 machines for
    // We write an empty string (parser should return Ok([]) for empty input or error)
    write!(f, "").unwrap();
    drop(f);

    let path_str = file_path.to_str().unwrap();
    let result = CodeGenerator::from_files(&[path_str]);
    assert!(result.is_err());
    // Could be Parse or NoMachines depending on how parser handles empty input
    assert!(
        matches!(
            result,
            Err(CodegenError::Parse(_)) | Err(CodegenError::NoMachines)
        ),
        "Expected Parse or NoMachines error"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// 14. from_files() — happy path: valid SMQL file on disk
#[test]
fn test_from_files_happy_path() {
    use std::io::Write;
    let dir = std::env::temp_dir().join("smql_codegen_test_happy");
    let _ = std::fs::create_dir_all(&dir);
    let file_path = dir.join("ticket.smql");
    let mut f = std::fs::File::create(&file_path).unwrap();
    write!(f, "{}", support_ticket_smql()).unwrap();
    drop(f);

    let path_str = file_path.to_str().unwrap();
    let gen = CodeGenerator::from_files(&[path_str]).unwrap();
    assert_eq!(gen.machines().len(), 1);
    assert_eq!(gen.machines()[0].name, "SupportTicket");

    let files = gen.generate_rust();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "support_ticket.rs");
    assert!(files[0].content.contains("pub mod support_ticket {"));

    let _ = std::fs::remove_dir_all(&dir);
}

// 15. from_files() — multiple files combined
#[test]
fn test_from_files_multiple_files() {
    use std::io::Write;
    let dir = std::env::temp_dir().join("smql_codegen_test_multi_files");
    let _ = std::fs::create_dir_all(&dir);

    let file1 = dir.join("ticket.smql");
    let mut f1 = std::fs::File::create(&file1).unwrap();
    write!(f1, "{}", support_ticket_smql()).unwrap();
    drop(f1);

    let file2 = dir.join("counter.smql");
    let mut f2 = std::fs::File::create(&file2).unwrap();
    write!(
        f2,
        r#"DEFINE MACHINE SimpleCounter (
        STATES {{ idle, done }}
        INITIAL STATE idle
        TERMINAL STATES {{ done }}
        TRANSITIONS {{ idle -> done {{}} }}
    )"#
    ).unwrap();
    drop(f2);

    let path1 = file1.to_str().unwrap();
    let path2 = file2.to_str().unwrap();
    let gen = CodeGenerator::from_files(&[path1, path2]).unwrap();
    assert_eq!(gen.machines().len(), 2);
    assert_eq!(gen.machines()[0].name, "SupportTicket");
    assert_eq!(gen.machines()[1].name, "SimpleCounter");

    let _ = std::fs::remove_dir_all(&dir);
}

// ============================================================
// Coverage gap tests: generator.rs — generate_combined_rust()
// ============================================================

// 16. generate_combined_rust() has header and multiple modules
#[test]
fn test_generate_combined_rust_header_and_structure() {
    let gen = CodeGenerator::from_source(order_smql()).unwrap();
    let combined = gen.generate_combined_rust();

    // Header comment
    assert!(combined.starts_with("// Auto-generated by smql-codegen. Do not edit.\n"));

    // Each machine module should be present
    assert!(combined.contains("pub mod order {"));
    assert!(combined.contains("pub mod order_item {"));
    assert!(combined.contains("pub mod payment {"));
}

// ============================================================
// Coverage gap tests: type_map.rs — smql_type_to_rust_with_enum()
// ============================================================

// 17. smql_type_to_rust_with_enum with Enum variant
#[test]
fn test_type_to_rust_with_enum_variant() {
    let ty = TypeDefinition::Enum(vec!["a".to_string(), "b".to_string()]);
    let result = smql_type_to_rust_with_enum(&ty, "MyPriority");
    assert_eq!(result, "MyPriority");
}

// 18. smql_type_to_rust_with_enum with non-Enum variant (falls through to smql_type_to_rust)
#[test]
fn test_type_to_rust_with_enum_non_enum() {
    assert_eq!(
        smql_type_to_rust_with_enum(&TypeDefinition::Text, "Ignored"),
        "String"
    );
    assert_eq!(
        smql_type_to_rust_with_enum(&TypeDefinition::Int, "Ignored"),
        "i64"
    );
    assert_eq!(
        smql_type_to_rust_with_enum(&TypeDefinition::Bool, "Ignored"),
        "bool"
    );
    assert_eq!(
        smql_type_to_rust_with_enum(&TypeDefinition::Float, "Ignored"),
        "f64"
    );
    assert_eq!(
        smql_type_to_rust_with_enum(&TypeDefinition::Blob, "Ignored"),
        "Vec<u8>"
    );
    assert_eq!(
        smql_type_to_rust_with_enum(&TypeDefinition::Json, "Ignored"),
        "serde_json::Value"
    );
    assert_eq!(
        smql_type_to_rust_with_enum(&TypeDefinition::Uuid, "Ignored"),
        "String"
    );
    assert_eq!(
        smql_type_to_rust_with_enum(&TypeDefinition::Date, "Ignored"),
        "String"
    );
    assert_eq!(
        smql_type_to_rust_with_enum(&TypeDefinition::DateTime, "Ignored"),
        "String"
    );
    assert_eq!(
        smql_type_to_rust_with_enum(&TypeDefinition::Duration, "Ignored"),
        "String"
    );
}

// ============================================================
// Coverage gap tests: rust_gen.rs — edge cases
// ============================================================

// 19. to_pascal_case with consecutive separators (empty parts filtered out)
#[test]
fn test_pascal_case_consecutive_separators() {
    // Double underscore: should skip the empty part
    assert_eq!(to_pascal_case("foo__bar"), "FooBar");
    // Leading underscore
    assert_eq!(to_pascal_case("_foo"), "Foo");
    // Trailing underscore
    assert_eq!(to_pascal_case("foo_"), "Foo");
    // Mixed separators
    assert_eq!(to_pascal_case("foo-bar_baz"), "FooBarBaz");
    // Spaces
    assert_eq!(to_pascal_case("foo bar"), "FooBar");
    // Triple separator
    assert_eq!(to_pascal_case("a___b"), "AB");
}

// 20. to_pascal_case with empty string
#[test]
fn test_pascal_case_empty_string() {
    assert_eq!(to_pascal_case(""), "");
}

// 21. to_snake_case with dashes and spaces
#[test]
fn test_snake_case_dashes_and_spaces() {
    assert_eq!(to_snake_case("foo-bar"), "foo_bar");
    assert_eq!(to_snake_case("foo bar"), "foo_bar");
    // "my-Machine" → dash becomes _, M adds _ → "my__machine"
    assert_eq!(to_snake_case("my-Machine"), "my__machine");
    assert_eq!(to_snake_case("ABC"), "a_b_c");
    assert_eq!(to_snake_case(""), "");
}

// 22. Machine with no terminal states — the "_ if false => true" branch
#[test]
fn test_no_terminal_states() {
    // A machine that has no terminal states declared
    // We construct the machine directly to bypass parser validation.
    use smql_ast::machine::MachineDefinition;

    let mut def = MachineDefinition::new("Forever".to_string(), "running".to_string());
    def.states.push(smql_ast::machine::StateDefinition::new("running".to_string()));
    def.states.push(smql_ast::machine::StateDefinition::new("paused".to_string()));
    def.terminal_states = vec![]; // No terminal states
    def.transitions.push(smql_ast::machine::TransitionDefinition::new(
        smql_ast::machine::TransitionSource::State("running".to_string()),
        "paused".to_string(),
    ));

    let output = smql_codegen::rust_gen::generate_machine_module(&def);

    // Should contain the fallback is_terminal (no terminal states)
    assert!(
        output.contains("_ if false => true"),
        "Expected fallback is_terminal for no terminal states, got:\n{}",
        output
    );
    assert!(output.contains("pub mod forever {"));
    assert!(output.contains("Running,"));
    assert!(output.contains("Paused,"));
}

// 23. Machine with no data fields — generate_data_struct with empty data
#[test]
fn test_machine_no_data_fields_via_direct_construction() {
    use smql_ast::machine::MachineDefinition;

    let mut def = MachineDefinition::new("Minimal".to_string(), "start".to_string());
    def.states.push(smql_ast::machine::StateDefinition::new("start".to_string()));
    def.states.push(smql_ast::machine::StateDefinition::new("end".to_string()));
    def.terminal_states = vec!["end".to_string()];
    def.transitions.push(smql_ast::machine::TransitionDefinition::new(
        smql_ast::machine::TransitionSource::State("start".to_string()),
        "end".to_string(),
    ));

    let output = smql_codegen::rust_gen::generate_machine_module(&def);

    // Empty data struct
    assert!(output.contains("pub struct MinimalData {"));
    // The struct should close immediately (no fields)
    assert!(output.contains("pub struct MinimalData {\n    }\n"));
    assert!(output.contains("pub struct Minimal;"));
    assert!(output.contains("Self::End"));
}

// 24. Enum type in smql_type_to_rust returns "String" (not a generated name)
#[test]
fn test_type_mapping_enum_returns_string() {
    let ty = TypeDefinition::Enum(vec!["x".to_string(), "y".to_string()]);
    // The base smql_type_to_rust maps Enum to "String" (the caller typically
    // uses smql_type_to_rust_with_enum for actual codegen)
    assert_eq!(smql_type_to_rust(&ty), "String");
}

// 25. Nested list/set/map types
#[test]
fn test_nested_type_mapping() {
    // List of lists
    let nested_list = TypeDefinition::List(Box::new(TypeDefinition::List(Box::new(
        TypeDefinition::Int,
    ))));
    assert_eq!(smql_type_to_rust(&nested_list), "Vec<Vec<i64>>");

    // Set of bools
    let set_of_bool = TypeDefinition::Set(Box::new(TypeDefinition::Bool));
    assert_eq!(smql_type_to_rust(&set_of_bool), "Vec<bool>");

    // Map with complex value
    let complex_map = TypeDefinition::Map(
        Box::new(TypeDefinition::Text),
        Box::new(TypeDefinition::List(Box::new(TypeDefinition::Float))),
    );
    assert_eq!(
        smql_type_to_rust(&complex_map),
        "std::collections::BTreeMap<String, Vec<f64>>"
    );
}

// 26. generate_rust() returns individual files with correct content
#[test]
fn test_generate_rust_individual_files() {
    let gen = CodeGenerator::from_source(support_ticket_smql()).unwrap();
    let files = gen.generate_rust();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "support_ticket.rs");
    assert!(files[0].content.contains("pub mod support_ticket {"));
    assert!(files[0].content.contains("pub struct SupportTicketData {"));
    assert!(files[0].content.contains("pub struct SupportTicket;"));
}

// 27. CodegenError Display formatting
#[test]
fn test_codegen_error_display() {
    let io_err = CodegenError::Io(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "file not found",
    ));
    assert!(format!("{}", io_err).contains("file not found"));

    let parse_err = CodegenError::Parse("bad syntax".to_string());
    assert_eq!(format!("{}", parse_err), "Parse error: bad syntax");

    let no_machines = CodegenError::NoMachines;
    assert_eq!(format!("{}", no_machines), "No machines found in input");
}

// 28. GeneratedFile struct fields are accessible
#[test]
fn test_generated_file_clone_debug() {
    let gen = CodeGenerator::from_source(support_ticket_smql()).unwrap();
    let files = gen.generate_rust();
    let file = files[0].clone();
    assert_eq!(file.path, "support_ticket.rs");
    assert!(!file.content.is_empty());
    // Debug impl should work
    let debug_str = format!("{:?}", file);
    assert!(debug_str.contains("GeneratedFile"));
}
