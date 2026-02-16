# Typed API

The SMQL SDK includes a typed layer that wraps the runtime client with compile-time safety. Instead of working with raw JSON and string state names, you work with Rust structs and enums generated from your SMQL machine definitions.

## Overview

The typed API consists of three pieces:

1. **`SmqlMachine` trait** -- implemented by a generated struct representing the machine.
2. **`SmqlState` trait** -- implemented by a generated enum representing the machine's states.
3. **`TypedInstance<M>`** -- a wrapper around `InstanceResponse` that deserializes data into the machine's data struct and provides typed access to the current state.

Code generation is covered in [Codegen](./codegen). This page focuses on using the generated types.

## Generated Types

Given this SMQL definition:

```sql
MACHINE SupportTicket {
    STATE open {
        ON_ENTER {
            REQUIRE subject: TEXT
            REQUIRE priority: INT
        }
    }
    STATE in_progress
    STATE resolved
    STATE closed TERMINAL

    open -> in_progress
    in_progress -> resolved
    resolved -> closed
    resolved -> open
}
```

The codegen produces:

```rust
/// Machine struct
pub struct SupportTicket;

/// State enum
#[derive(Debug, Clone, PartialEq)]
pub enum SupportTicketState {
    Open,
    InProgress,
    Resolved,
    Closed,
}

/// Data struct
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SupportTicketData {
    pub subject: String,
    pub priority: i64,
}
```

## SmqlMachine Trait

The `SmqlMachine` trait associates a machine with its state enum and data struct.

```rust
pub trait SmqlMachine {
    type State: SmqlState;
    type Data: serde::Serialize + serde::de::DeserializeOwned;

    fn machine_name() -> &'static str;
}
```

The generated implementation:

```rust
impl SmqlMachine for SupportTicket {
    type State = SupportTicketState;
    type Data = SupportTicketData;

    fn machine_name() -> &'static str {
        "SupportTicket"
    }
}
```

## SmqlState Trait

The `SmqlState` trait maps between Rust enum variants and SMQL state name strings.

```rust
pub trait SmqlState: Sized {
    fn from_str(s: &str) -> Option<Self>;
    fn as_str(&self) -> &'static str;
}
```

The generated implementation maps variant names to their SMQL equivalents:

```rust
impl SmqlState for SupportTicketState {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "in_progress" => Some(Self::InProgress),
            "resolved" => Some(Self::Resolved),
            "closed" => Some(Self::Closed),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::Resolved => "resolved",
            Self::Closed => "closed",
        }
    }
}
```

## TypedInstance

`TypedInstance<M>` wraps an `InstanceResponse` and provides typed access.

```rust
pub struct TypedInstance<M: SmqlMachine> {
    pub id: String,
    pub state: M::State,
    pub data: M::Data,
    pub version: u64,
    pub created_at: String,
    pub updated_at: String,
    // ...
}
```

### Creating a TypedInstance

Construct a `TypedInstance` from an `InstanceResponse`:

```rust
use smql_sdk::TypedInstance;

let response = client.get_instance("01HQXYZ...").await?;
let ticket: TypedInstance<SupportTicket> = TypedInstance::try_from(response)?;
```

The conversion deserializes `response.data` into `SupportTicketData` and parses `response.state` into `SupportTicketState`. It returns `SdkError::Deserialize` if either fails.

### Accessing Fields

```rust
let ticket: TypedInstance<SupportTicket> = TypedInstance::try_from(response)?;

// Typed state -- no string comparisons needed
match ticket.state {
    SupportTicketState::Open => println!("Ticket is open"),
    SupportTicketState::InProgress => println!("Being worked on"),
    SupportTicketState::Resolved => println!("Resolved"),
    SupportTicketState::Closed => println!("Closed"),
}

// Typed data -- direct field access, no JSON wrangling
println!("Subject: {}", ticket.data.subject);
println!("Priority: {}", ticket.data.priority);
```

### Pattern Matching Example

```rust
fn needs_attention(ticket: &TypedInstance<SupportTicket>) -> bool {
    ticket.state == SupportTicketState::Open && ticket.data.priority == 1
}
```

## Working with Find Results

Convert a vector of `InstanceResponse` into typed instances:

```rust
let responses = client.find("SupportTicket")
    .in_state("open")
    .execute()
    .await?;

let tickets: Vec<TypedInstance<SupportTicket>> = responses
    .into_iter()
    .map(TypedInstance::try_from)
    .collect::<Result<Vec<_>, _>>()?;

for ticket in &tickets {
    println!("[{}] {} (priority {})",
        ticket.state.as_str(), ticket.data.subject, ticket.data.priority);
}
```

## Working with Transitions

After a transition, convert the `TransitionResponse` into a typed instance:

```rust
use smql_sdk::TransitionOptions;

let result = client.transition(
    &ticket.id,
    SupportTicketState::InProgress.as_str(),
    TransitionOptions::default(),
).await?;

let updated: TypedInstance<SupportTicket> = TypedInstance::try_from(result.instance)?;
assert_eq!(updated.state, SupportTicketState::InProgress);
```

Use `SmqlState::as_str()` to pass the state name to client methods, keeping everything type-safe.

## Type Mapping

The codegen maps SMQL types to Rust types in the data struct:

| SMQL Type | Rust Type |
|-----------|-----------|
| `TEXT` | `String` |
| `INT` | `i64` |
| `FLOAT` | `f64` |
| `BOOL` | `bool` |
| `MONEY` | `(i64, String)` |
| `UUID` | `String` |
| `TIMESTAMP` | `String` |
| `LIST` | `Vec<serde_json::Value>` |
| `MAP` | `serde_json::Map<String, Value>` |

See [Codegen](./codegen) for details on how to generate these types from SMQL files.
