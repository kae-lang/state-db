# Code Generation

The SMQL SDK includes a code generator that reads `.smql` files and produces Rust source code implementing the `SmqlMachine` and `SmqlState` traits. This gives you compile-time type safety for your state machines.

## Overview

The codegen reads SMQL machine definitions, parses them, and emits Rust code containing:

- A unit struct for each machine (e.g., `pub struct Order;`)
- A state enum with one variant per state (e.g., `OrderState::Pending`)
- A data struct with typed fields derived from `ON_ENTER` requirements
- Trait implementations for `SmqlMachine` and `SmqlState`

## Using CodeGenerator

### Basic Usage

```rust
use smql_sdk::codegen::CodeGenerator;

fn main() {
    let code = CodeGenerator::from_files(&["machines/order.smql", "machines/ticket.smql"])
        .generate_rust();

    println!("{}", code);
}
```

### Writing to a File

A common pattern is to use a `build.rs` build script:

```rust
// build.rs
use smql_sdk::codegen::CodeGenerator;
use std::fs;
use std::path::Path;

fn main() {
    let code = CodeGenerator::from_files(&[
        "machines/order.smql",
        "machines/support_ticket.smql",
    ])
    .generate_rust();

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("smql_generated.rs");
    fs::write(&dest, code).unwrap();

    println!("cargo:rerun-if-changed=machines/");
}
```

Then include the generated code in your application:

```rust
// src/machines.rs
include!(concat!(env!("OUT_DIR"), "/smql_generated.rs"));
```

### API Reference

#### `CodeGenerator::from_files`

Create a code generator from one or more SMQL file paths. Each file is read and parsed.

```rust
pub fn from_files(paths: &[&str]) -> CodeGenerator
```

Panics if a file cannot be read or contains invalid SMQL.

#### `generate_rust`

Produce the generated Rust source code as a `String`.

```rust
pub fn generate_rust(&self) -> String
```

## Type Mapping

SMQL types in `REQUIRE` declarations are mapped to Rust types:

| SMQL Type | Rust Type | Notes |
|-----------|-----------|-------|
| `TEXT` | `String` | |
| `INT` | `i64` | |
| `FLOAT` | `f64` | |
| `BOOL` | `bool` | |
| `MONEY` | `(i64, String)` | Tuple of amount in minor units and currency code |
| `UUID` | `String` | Stored as string representation |
| `TIMESTAMP` | `String` | ISO 8601 string |
| `LIST` | `Vec<serde_json::Value>` | Heterogeneous list |
| `MAP` | `serde_json::Map<String, Value>` | JSON object |

## Generated Code Structure

For this SMQL definition:

```sql
MACHINE Order {
    STATE pending {
        ON_ENTER {
            REQUIRE item: TEXT
            REQUIRE quantity: INT
            REQUIRE total: MONEY
        }
    }
    STATE confirmed
    STATE shipped
    STATE delivered TERMINAL
    STATE cancelled TERMINAL

    pending -> confirmed
    confirmed -> shipped
    shipped -> delivered
    pending -> cancelled
    confirmed -> cancelled
}
```

The codegen produces:

```rust
// --- Order ---

pub struct Order;

#[derive(Debug, Clone, PartialEq)]
pub enum OrderState {
    Pending,
    Confirmed,
    Shipped,
    Delivered,
    Cancelled,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrderData {
    pub item: String,
    pub quantity: i64,
    pub total: (i64, String),
}

impl SmqlMachine for Order {
    type State = OrderState;
    type Data = OrderData;

    fn machine_name() -> &'static str {
        "Order"
    }
}

impl SmqlState for OrderState {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "confirmed" => Some(Self::Confirmed),
            "shipped" => Some(Self::Shipped),
            "delivered" => Some(Self::Delivered),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Shipped => "shipped",
            Self::Delivered => "delivered",
            Self::Cancelled => "cancelled",
        }
    }
}
```

## Naming Conventions

The codegen applies the following conventions:

| SMQL Name | Rust Name | Example |
|-----------|-----------|---------|
| Machine name | Struct name (PascalCase) | `SupportTicket` stays `SupportTicket` |
| State name | Enum variant (PascalCase) | `in_progress` becomes `InProgress` |
| Data field | Struct field (snake_case) | `trackingNumber` becomes `tracking_number` |
| State enum | `{Machine}State` | `OrderState` |
| Data struct | `{Machine}Data` | `OrderData` |

Machine names that are already PascalCase are preserved as-is (e.g., `SupportTicket` stays `SupportTicket`, not `Supportticket`).

## Multiple Machines

When passing multiple files to `from_files`, all machines are generated into a single output string. Each machine gets its own struct, state enum, and data struct.

```rust
let code = CodeGenerator::from_files(&[
    "machines/order.smql",
    "machines/invoice.smql",
    "machines/support_ticket.smql",
])
.generate_rust();
```

This produces `Order`, `OrderState`, `OrderData`, `Invoice`, `InvoiceState`, `InvoiceData`, `SupportTicket`, `SupportTicketState`, `SupportTicketData`, and all the corresponding trait implementations.

## Integration with build.rs

A complete `build.rs` example with directory scanning:

```rust
use smql_sdk::codegen::CodeGenerator;
use std::{fs, path::Path};

fn main() {
    let machines_dir = "machines";
    let mut files: Vec<String> = Vec::new();

    for entry in fs::read_dir(machines_dir).expect("machines/ directory not found") {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().map_or(false, |ext| ext == "smql") {
            files.push(path.to_string_lossy().to_string());
        }
    }

    let file_refs: Vec<&str> = files.iter().map(|s| s.as_str()).collect();
    let code = CodeGenerator::from_files(&file_refs).generate_rust();

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest = Path::new(&out_dir).join("smql_generated.rs");
    fs::write(&dest, code).unwrap();

    println!("cargo:rerun-if-changed={}", machines_dir);
}
```

Then in your crate:

```rust
mod machines {
    use smql_sdk::{SmqlMachine, SmqlState};
    include!(concat!(env!("OUT_DIR"), "/smql_generated.rs"));
}

use machines::*;
```
