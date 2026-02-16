use smql_ast::types::TypeDefinition;

/// Map an SMQL type to a Rust type string.
pub fn smql_type_to_rust(ty: &TypeDefinition) -> String {
    match ty {
        TypeDefinition::Text => "String".to_string(),
        TypeDefinition::Int => "i64".to_string(),
        TypeDefinition::Float => "f64".to_string(),
        TypeDefinition::Bool => "bool".to_string(),
        TypeDefinition::Uuid => "String".to_string(),
        TypeDefinition::Date => "String".to_string(),
        TypeDefinition::DateTime => "String".to_string(),
        TypeDefinition::Duration => "String".to_string(),
        TypeDefinition::Enum(_) => "String".to_string(), // replaced with generated enum name
        TypeDefinition::Ref(_) => "String".to_string(),
        TypeDefinition::List(inner) => format!("Vec<{}>", smql_type_to_rust(inner)),
        TypeDefinition::Set(inner) => format!("Vec<{}>", smql_type_to_rust(inner)),
        TypeDefinition::Map(k, v) => {
            format!(
                "std::collections::BTreeMap<{}, {}>",
                smql_type_to_rust(k),
                smql_type_to_rust(v)
            )
        }
        TypeDefinition::Blob => "Vec<u8>".to_string(),
        TypeDefinition::Money(_) => "(i64, String)".to_string(),
        TypeDefinition::Json => "serde_json::Value".to_string(),
    }
}

/// Map an SMQL type to a Rust type string, using a generated enum name for ENUM fields.
pub fn smql_type_to_rust_with_enum(ty: &TypeDefinition, enum_name: &str) -> String {
    match ty {
        TypeDefinition::Enum(_) => enum_name.to_string(),
        other => smql_type_to_rust(other),
    }
}
