use smql_ast::machine::MachineDefinition;
use smql_ast::types::{Constraint, TypeDefinition};

use crate::type_map;

/// Generate Rust source code for a single machine definition.
pub fn generate_machine_module(def: &MachineDefinition) -> String {
    let mut out = String::new();

    let mod_name = to_snake_case(&def.name);
    let struct_name = to_pascal_case(&def.name);

    out.push_str(&format!("pub mod {} {{\n", mod_name));
    out.push_str("    use serde::{Deserialize, Serialize};\n\n");

    // Generate inline enums for ENUM fields
    for field in &def.data {
        if let TypeDefinition::Enum(variants) = &field.field_type {
            let enum_name = field_enum_name(&struct_name, &field.name);
            out.push_str(&generate_field_enum(&enum_name, variants));
        }
    }

    // Generate state enum
    out.push_str(&generate_state_enum(def));

    // Generate data struct
    out.push_str(&generate_data_struct(def));

    // Generate machine marker struct + SmqlMachine impl
    out.push_str(&generate_machine_marker(def));

    out.push_str("}\n");
    out
}

fn generate_field_enum(name: &str, variants: &[String]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\n"
    ));
    out.push_str(&format!("    pub enum {} {{\n", name));
    for v in variants {
        let variant = to_pascal_case(v);
        out.push_str(&format!(
            "        #[serde(rename = \"{}\")]\n        {},\n",
            v, variant
        ));
    }
    out.push_str("    }\n\n");
    out
}

fn generate_state_enum(def: &MachineDefinition) -> String {
    let mut out = String::new();
    out.push_str("    #[derive(Debug, Clone, PartialEq)]\n    pub enum State {\n");
    for state in &def.states {
        out.push_str(&format!("        {},\n", to_pascal_case(&state.name)));
    }
    out.push_str("    }\n\n");

    // SmqlState impl
    out.push_str("    impl State {\n");

    // from_str
    out.push_str("        pub fn from_name(s: &str) -> Option<Self> {\n");
    out.push_str("            match s {\n");
    for state in &def.states {
        out.push_str(&format!(
            "                \"{}\" => Some(Self::{}),\n",
            state.name,
            to_pascal_case(&state.name)
        ));
    }
    out.push_str("                _ => None,\n");
    out.push_str("            }\n");
    out.push_str("        }\n\n");

    // as_str
    out.push_str("        pub fn as_str(&self) -> &str {\n");
    out.push_str("            match self {\n");
    for state in &def.states {
        out.push_str(&format!(
            "                Self::{} => \"{}\",\n",
            to_pascal_case(&state.name),
            state.name
        ));
    }
    out.push_str("            }\n");
    out.push_str("        }\n\n");

    // is_terminal
    out.push_str("        pub fn is_terminal(&self) -> bool {\n");
    out.push_str("            matches!(self, ");
    let terminals: Vec<String> = def
        .terminal_states
        .iter()
        .map(|s| format!("Self::{}", to_pascal_case(s)))
        .collect();
    if terminals.is_empty() {
        out.push_str("_ if false => true");
    } else {
        out.push_str(&terminals.join(" | "));
    }
    out.push_str(")\n");
    out.push_str("        }\n");
    out.push_str("    }\n\n");
    out
}

fn generate_data_struct(def: &MachineDefinition) -> String {
    let mut out = String::new();
    let struct_name = to_pascal_case(&def.name);
    let data_name = format!("{}Data", struct_name);

    out.push_str(&format!(
        "    #[derive(Debug, Clone, Serialize, Deserialize)]\n    pub struct {} {{\n",
        data_name
    ));

    for field in &def.data {
        let is_optional = field
            .constraints
            .iter()
            .any(|c| matches!(c, Constraint::Optional));

        let rust_type = if let TypeDefinition::Enum(_) = &field.field_type {
            let enum_name = field_enum_name(&struct_name, &field.name);
            enum_name
        } else {
            type_map::smql_type_to_rust(&field.field_type)
        };

        let final_type = if is_optional {
            format!("Option<{}>", rust_type)
        } else {
            rust_type
        };

        out.push_str(&format!("        pub {}: {},\n", field.name, final_type));
    }

    out.push_str("    }\n\n");
    out
}

fn generate_machine_marker(def: &MachineDefinition) -> String {
    let mut out = String::new();
    let struct_name = to_pascal_case(&def.name);
    let data_name = format!("{}Data", struct_name);

    out.push_str(&format!("    pub struct {};\n\n", struct_name));

    out.push_str(&format!("    impl {} {{\n", struct_name));
    out.push_str(&format!(
        "        pub const MACHINE_NAME: &'static str = \"{}\";\n",
        def.name
    ));
    out.push_str(&format!("        pub type Data = {};\n", data_name));
    out.push_str(&format!("        pub type MachineState = State;\n"));
    out.push_str("    }\n");
    out
}

/// Convert a string to PascalCase.
/// Handles snake_case (in_progress → InProgress), already-PascalCase (SupportTicket → SupportTicket),
/// and lowercase (open → Open).
pub fn to_pascal_case(s: &str) -> String {
    if s.contains('_') || s.contains('-') || s.contains(' ') {
        // Split on separators and capitalize each part
        s.split(|c: char| c == '_' || c == '-' || c == ' ')
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => {
                        let mut result = first.to_uppercase().to_string();
                        result.extend(chars);
                        result
                    }
                }
            })
            .collect()
    } else if s.chars().next().map_or(false, |c| c.is_lowercase()) {
        // Simple lowercase word: capitalize first letter
        let mut chars = s.chars();
        match chars.next() {
            None => String::new(),
            Some(first) => {
                let mut result = first.to_uppercase().to_string();
                result.extend(chars);
                result
            }
        }
    } else {
        // Already PascalCase or mixed case: return as-is
        s.to_string()
    }
}

/// Convert a string to snake_case.
pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.extend(ch.to_lowercase());
        } else if ch == '-' || ch == ' ' {
            result.push('_');
        } else {
            result.push(ch);
        }
    }
    result
}

/// Generate the enum name for a field's ENUM type.
fn field_enum_name(machine_pascal: &str, field_name: &str) -> String {
    format!("{}{}", machine_pascal, to_pascal_case(field_name))
}
