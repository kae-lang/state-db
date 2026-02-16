# TRY TRANSITION

`TRY TRANSITION` attempts a transition but returns success even if the guard fails. Instead of an error, it returns `transitioned: false`.

## Syntax

```sql
TRY TRANSITION "instance_id" TO target_state
TRY TRANSITION "instance_id" TO target_state AS { id: "u1", role: "admin" }
```

All the same clauses as `TRANSITION` are supported (AS, WITH, MEMO).

## Response (Success)

```json
{
  "success": true,
  "result": {
    "transitioned": true,
    "from_state": "open",
    "to_state": "triaged",
    "instance": { ... }
  }
}
```

## Response (Guard Failed)

```json
{
  "success": true,
  "result": {
    "transitioned": false
  }
}
```

Note that `success` is `true` in both cases -- the command itself succeeded, the transition just didn't happen.

## Use Cases

- **Conditional workflows**: try to advance and proceed based on the result
- **Polling patterns**: periodically try to transition, handle both outcomes
- **Idempotent operations**: safe to retry without error handling for guard failures

## SDK

```rust
let result = client.try_transition(
    "01J5...",
    "resolved",
    TransitionOptions::default(),
).await?;

match result {
    Some(tr) => println!("Transitioned: {} -> {}", tr.from_state, tr.to_state),
    None => println!("Guard failed, stayed in current state"),
}
```
