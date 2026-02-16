# Quick Start

Get SMQL running and manage your first state machine in 5 minutes.

## Build from Source

```bash
git clone <repo-url> && cd smql-engine
cargo build --release
```

The binary is at `target/release/smql`.

## Start the Server

```bash
smql serve --bind 127.0.0.1:4200
```

This starts the HTTP server with in-memory storage. For persistent storage, use RocksDB:

```bash
smql serve --bind 127.0.0.1:4200 --storage ./data
```

## Define a Machine

::: code-group
```bash [curl]
curl -X POST http://localhost:4200/execute \
  -H 'Content-Type: application/json' \
  -d '{
    "smql": "DEFINE MACHINE Task ( DATA { title: TEXT -> REQUIRED, assignee: TEXT -> OPTIONAL } STATES { todo, doing, done } INITIAL STATE todo TERMINAL STATES { done } TRANSITIONS { todo -> doing {} doing -> done {} } )"
  }'
```

```bash [REPL]
smql repl
> DEFINE MACHINE Task (
    DATA { title: TEXT -> REQUIRED, assignee: TEXT -> OPTIONAL }
    STATES { todo, doing, done }
    INITIAL STATE todo
    TERMINAL STATES { done }
    TRANSITIONS {
      todo -> doing {}
      doing -> done {}
    }
  )
```

```rust [SDK]
use smql_sdk::SmqlClient;

let client = SmqlClient::new("http://localhost:4200")?;
client.define_machine(r#"
  DEFINE MACHINE Task (
    DATA { title: TEXT -> REQUIRED, assignee: TEXT -> OPTIONAL }
    STATES { todo, doing, done }
    INITIAL STATE todo
    TERMINAL STATES { done }
    TRANSITIONS {
      todo -> doing {}
      doing -> done {}
    }
  )
"#).await?;
```
:::

Response:
```json
{
  "success": true,
  "result": { "action": "machine_defined" }
}
```

## Spawn an Instance

::: code-group
```bash [curl]
curl -X POST http://localhost:4200/execute \
  -H 'Content-Type: application/json' \
  -d '{
    "smql": "SPAWN Task { title: \"Write docs\", assignee: \"alice\" }"
  }'
```

```bash [REPL]
> SPAWN Task { title: "Write docs", assignee: "alice" }
```

```rust [SDK]
let instance = client.spawn("Task", serde_json::json!({
    "title": "Write docs",
    "assignee": "alice"
})).await?;
println!("ID: {}", instance.id);   // ULID like "01J5..."
println!("State: {}", instance.state); // "todo"
```
:::

Response:
```json
{
  "success": true,
  "result": {
    "id": "01J5X7K2P3Q4R5S6T7U8V9W0XY",
    "machine": "Task",
    "state": "todo",
    "data": { "title": "Write docs", "assignee": "alice" },
    "created_at": "2026-02-16T10:00:00Z",
    "updated_at": "2026-02-16T10:00:00Z",
    "state_entered_at": "2026-02-16T10:00:00Z",
    "trail_length": 1,
    "version": 1
  }
}
```

## Transition

::: code-group
```bash [curl]
curl -X POST http://localhost:4200/execute \
  -H 'Content-Type: application/json' \
  -d '{
    "smql": "TRANSITION Task \"01J5X7K2P3Q4R5S6T7U8V9W0XY\" TO doing"
  }'
```

```bash [REPL]
> TRANSITION Task "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO doing
```

```rust [SDK]
use smql_sdk::types::TransitionOptions;

let result = client.transition(
    "01J5X7K2P3Q4R5S6T7U8V9W0XY",
    "doing",
    TransitionOptions::default(),
).await?;
println!("{} -> {}", result.from_state, result.to_state);
```
:::

## Query

::: code-group
```bash [curl]
curl -X POST http://localhost:4200/execute \
  -H 'Content-Type: application/json' \
  -d '{ "smql": "FIND Task WHERE STATE IS doing" }'
```

```bash [REPL]
> FIND Task WHERE STATE IS doing
```

```rust [SDK]
let results = client.find("Task")
    .in_state("doing")
    .execute()
    .await?;
```
:::

## Try the REPL

The interactive REPL supports multiline input and dot commands:

```bash
smql repl
> .help        # Show available commands
> .machines    # List registered machines
> .quit        # Exit
```

## Next Steps

- [Key Concepts](./key-concepts) — understand the data model
- [DEFINE MACHINE](../language/define-machine) — full syntax reference
- [Support Ticket Guide](../guides/support-ticket) — complete walkthrough
