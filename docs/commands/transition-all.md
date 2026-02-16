# TRANSITION ALL

`TRANSITION ALL` transitions all matching instances of a machine at once.

## Syntax

```sql
TRANSITION ALL MachineName WHERE <predicate> TO target_state
```

## Example

```sql
TRANSITION ALL SupportTicket WHERE STATE IS resolved TO closed
```

This finds all `SupportTicket` instances in the `resolved` state and transitions each one to `closed`.

## Guards

Each instance is evaluated individually against the transition's guards. Instances that fail the guard are skipped.

::: info
`TRANSITION ALL` is useful for batch operations like closing all resolved tickets or cancelling all pending orders.
:::
