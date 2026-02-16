# TimerManager Internals

The `TimerManager` in `smql-timer` handles timeout transitions. When a state defines `TIMEOUT 24h -> escalated`, the engine schedules a timer that fires after 24 hours. If the instance is still in that state when the timer fires, the engine transitions it unconditionally.

## Data Structures

The timer manager uses a dual-index design for efficient scheduling and cancellation:

```rust
pub struct TimerManager {
    /// Ordered by deadline — poll scans from the front
    deadlines: Mutex<BTreeMap<Instant, Vec<TimerEntry>>>,

    /// Reverse index — O(1) lookup for cancellation
    keys: Mutex<HashMap<TimerKey, Instant>>,
}
```

### TimerEntry

```rust
pub struct TimerEntry {
    pub instance_id: InstanceId,
    pub machine: String,
    pub from_state: String,
    pub to_state: String,
}
```

A timer entry records everything needed to execute the transition when the timer fires: which instance, which machine, what state it should be leaving, and what state it should be entering.

### TimerKey

```rust
pub type TimerKey = (InstanceId, String); // (instance_id, from_state)
```

The key is `(instance_id, from_state)`. An instance can have at most one active timer per state. If the same state has multiple timeout transitions, they would need to be modeled differently (currently the last one wins).

## Operations

### schedule

```rust
pub fn schedule(&self, entry: TimerEntry, duration: Duration) {
    let deadline = Instant::now() + duration;
    let key = (entry.instance_id.clone(), entry.from_state.clone());

    let mut deadlines = self.deadlines.lock();
    let mut keys = self.keys.lock();

    // Cancel any existing timer for this key
    if let Some(old_deadline) = keys.remove(&key) {
        if let Some(entries) = deadlines.get_mut(&old_deadline) {
            entries.retain(|e| e.instance_id != key.0 || e.from_state != key.1);
            if entries.is_empty() {
                deadlines.remove(&old_deadline);
            }
        }
    }

    // Insert the new timer
    deadlines.entry(deadline).or_default().push(entry);
    keys.insert(key, deadline);
}
```

Scheduling first cancels any existing timer for the same key (idempotent reschedule), then inserts into both indexes. The `BTreeMap` keeps entries ordered by deadline, so polling only needs to check the front.

### cancel

```rust
pub fn cancel(&self, instance_id: &InstanceId, from_state: &str) {
    let key = (instance_id.clone(), from_state.to_string());

    let mut deadlines = self.deadlines.lock();
    let mut keys = self.keys.lock();

    if let Some(deadline) = keys.remove(&key) {
        if let Some(entries) = deadlines.get_mut(&deadline) {
            entries.retain(|e| e.instance_id != *instance_id || e.from_state != from_state);
            if entries.is_empty() {
                deadlines.remove(&deadline);
            }
        }
    }
}
```

Cancellation is O(1) lookup in the `HashMap` to find the deadline, then a linear scan within that deadline's entry vector (typically 1-2 entries per deadline). Without the reverse index, cancellation would require scanning the entire `BTreeMap`.

### poll_expired

```rust
pub fn poll_expired(&self) -> Vec<TimerEntry> {
    let now = Instant::now();
    let mut deadlines = self.deadlines.lock();
    let mut keys = self.keys.lock();
    let mut expired = Vec::new();

    // BTreeMap is sorted — split off everything <= now
    let remaining = deadlines.split_off(&(now + Duration::from_nanos(1)));
    for (_deadline, entries) in deadlines.drain() {
        for entry in entries {
            let key = (entry.instance_id.clone(), entry.from_state.clone());
            keys.remove(&key);
            expired.push(entry);
        }
    }
    *deadlines = remaining;

    expired
}
```

`poll_expired` uses `BTreeMap::split_off` to efficiently extract all entries with deadlines at or before the current time. This is O(k + log n) where k is the number of expired entries and n is the total number of entries. The remaining entries stay in the map.

### cancel_on_exit

When the engine transitions an instance out of a state, it calls `cancel(instance_id, old_state)`. This ensures that timers do not fire for states the instance has already left.

## Integration with the Engine

The engine polls the timer manager on a regular interval using `tokio::time::interval`:

```rust
let mut interval = tokio::time::interval(Duration::from_secs(1));
loop {
    interval.tick().await;
    let expired = timer_manager.poll_expired();
    for entry in expired {
        // Transition with actor = "System", no guards
        engine.transition_timeout(
            &entry.instance_id,
            &entry.to_state,
        ).await;
    }
}
```

### Timeout Transition Semantics

Timeout transitions differ from normal transitions in several ways:

| Property | Normal Transition | Timeout Transition |
|---|---|---|
| Actor | Provided by caller | `"System"` |
| Guards | Evaluated, must all pass | **Bypassed entirely** |
| BEFORE hooks | Executed, can reject | Executed, can reject |
| Trigger | Explicit command | Timer expiry |

Guard bypassing is intentional. A timeout represents an unconditional deadline: "if nothing happens within 24 hours, escalate." If the guard checked `priority == "high"`, low-priority tickets would never escalate, defeating the purpose of the timeout.

## Limitations

### No Persistence

Timers are stored in memory only. If the server restarts, all scheduled timers are lost. Instances that should have timed out will remain in their current state until something else transitions them.

Timer persistence is planned and will require a storage backend extension. The design would store timer entries alongside instances and reload them on startup, adjusting deadlines based on elapsed time.

### SmqlDuration Display

`SmqlDuration::from_hours(24)` displays as `"1d"` not `"24h"`. This is a normalization behavior in the display formatter, not a bug. Internally the duration is stored in seconds and the display picks the most readable unit.

### Single Timer per (Instance, State)

The current design supports one timer per `(instance_id, from_state)` pair. If a state had multiple timeout transitions to different target states, only the last one scheduled would be active. In practice, machines are designed with a single timeout per state.
