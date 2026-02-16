use smql_ast::types::TypeDefinition;
use smql_codegen::rust_gen::{to_pascal_case, to_snake_case};
use smql_codegen::type_map::smql_type_to_rust;
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
