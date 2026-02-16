# Timeouts

The `TIMEOUT` clause defines an automatic transition that fires after a specified duration.

## Syntax

```sql
source -> target {
  TIMEOUT : <duration> -> <timeout_target_state>
}
```

## Example

```sql
in_progress -> waiting_on_customer {
  TIMEOUT : 72h -> resolved
}
```

When an instance enters `waiting_on_customer`, a 72-hour timer starts. If the instance is still in that state after 72 hours, it automatically transitions to `resolved`.

## Timer Behavior

### Start
Timers start when an instance enters a state with a timeout-bearing transition.

### Cancel on Exit
When an instance leaves a state, any active timer for that state is automatically cancelled. This is "cancel-on-exit" semantics.

### Guard-Free Execution
Timeout transitions bypass guards. They execute unconditionally when the timer fires. The actor for timeout transitions is `"System"`.

### Multiple Timeouts

Different transitions from the same state can have different timeouts:

```sql
placed -> paid {
  TIMEOUT : 24h -> cancelled
}

placed -> expired {
  TIMEOUT : 48h -> expired
}
```

## Duration Syntax

| Format | Duration |
|--------|----------|
| `30s` | 30 seconds |
| `5m` | 5 minutes |
| `1h` | 1 hour |
| `72h` | 72 hours |
| `7d` | 7 days |
| `1w` | 1 week |

::: info
Internally, the timer system uses a `BTreeMap<deadline, Vec<entry>>` for efficient scheduling and a `HashMap<key, deadline>` for O(1) cancellation.
:::

::: warning
Timers are currently in-memory only. If the server restarts, pending timers are lost. Timer persistence is planned for a future release.
:::
