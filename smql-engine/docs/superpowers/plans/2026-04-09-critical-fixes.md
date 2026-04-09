# Critical Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 11 critical issues in the SMQL engine: non-atomic writes, TOCTOU concurrency bugs, silent error swallowing, and unsafe unwrap() calls.

**Architecture:** Changes touch 4 files primarily: `smql-storage/src/traits.rs` (new trait method), `smql-storage/src/memory.rs` (implement new method), `smql-storage/src/rocksdb.rs` (implement new method + replace unwraps), and `smql-engine/src/engine.rs` (use atomic spawn, propagate errors, fix deferred spawns). We also fix a division-by-zero edge case in `smql-engine/src/eval.rs`.

**Tech Stack:** Rust, async-trait, DashMap, RocksDB WriteBatch, tokio, serde_json

---

## File Structure

| File | Action | Responsibility |
|------|--------|----------------|
| `crates/smql-storage/src/traits.rs` | Modify | Add `spawn_instance` method (atomic instance+trail+event+idempotency) |
| `crates/smql-storage/src/memory.rs` | Modify | Implement `spawn_instance` |
| `crates/smql-storage/src/rocksdb.rs` | Modify | Implement `spawn_instance` with WriteBatch; replace 41 `unwrap()` with cached CF handles; use version-checked WriteBatch for `update_instance` |
| `crates/smql-engine/src/engine.rs` | Modify | Use `spawn_instance`; propagate errors from cascade/deferred spawns/events/sagas; fix deferred spawn version tracking; make transaction rollback use atomic writes |
| `crates/smql-engine/src/eval.rs` | Modify | Fix Float/Int division-by-zero edge case |

---

### Task 1: Add `spawn_instance` to Storage Trait (Atomic Spawn)

**Why:** Currently `store_instance` and `append_trail_entry` are separate calls. Crash between them = instance with no audit trail. We need a single atomic method that writes instance + trail + event + idempotency key together.

**Files:**
- Modify: `crates/smql-storage/src/traits.rs:12-192`
- Modify: `crates/smql-storage/src/memory.rs:114-140`
- Modify: `crates/smql-storage/src/rocksdb.rs:297-349`

- [ ] **Step 1: Add `spawn_instance` to the `Storage` trait**

In `crates/smql-storage/src/traits.rs`, add after `store_instance`:

```rust
/// Atomically store a new instance with its initial trail entry, event, and optional idempotency key.
/// All writes succeed or none do. This prevents orphaned instances without trail entries.
async fn spawn_instance(
    &self,
    instance: &Instance,
    trail_entry: &TrailEntry,
    event: Option<&crate::instance::StoredEvent>,
    idempotency_key: Option<(&str, &[u8], chrono::DateTime<chrono::Utc>)>,
) -> SmqlResult<()> {
    // Default implementation for backwards compatibility: sequential calls
    self.store_instance(instance).await?;
    self.append_trail_entry(trail_entry).await?;
    if let Some(evt) = event {
        let _ = self.store_event(evt).await;
    }
    if let Some((key, response, expires_at)) = idempotency_key {
        let _ = self.store_idempotency(key, response, expires_at).await;
    }
    Ok(())
}
```

- [ ] **Step 2: Implement `spawn_instance` in MemoryStorage**

In `crates/smql-storage/src/memory.rs`, add the implementation inside `impl Storage for MemoryStorage`. The memory backend can do a simple sequential write (since DashMap is in-process and crash = data loss anyway), but still checks for duplicates upfront:

```rust
async fn spawn_instance(
    &self,
    instance: &Instance,
    trail_entry: &TrailEntry,
    event: Option<&crate::instance::StoredEvent>,
    idempotency_key: Option<(&str, &[u8], chrono::DateTime<chrono::Utc>)>,
) -> SmqlResult<()> {
    // Check duplicate first
    let id_str = instance.id.as_str();
    if self.instances.contains_key(&id_str) {
        return Err(SmqlError::Conflict {
            message: format!("Instance '{}' already exists", id_str),
            hint: None,
        });
    }

    // Store instance + indices
    self.instances.insert(id_str.clone(), instance.clone());
    self.add_to_state_index(&instance.machine, &instance.state, &id_str);
    self.add_to_machine_index(&instance.machine, &id_str);
    self.trails.insert(id_str.clone(), RwLock::new(vec![trail_entry.clone()]));

    if let Some(parent_id) = &instance.parent_id {
        self.parent_index
            .entry(parent_id.as_str())
            .or_default()
            .insert(id_str);
    }

    if let Some(evt) = event {
        let mut events = self.events.write().unwrap();
        events.push(evt.clone());
    }

    if let Some((key, response, expires_at)) = idempotency_key {
        self.idempotency.insert(key.to_string(), (response.to_vec(), expires_at));
    }

    Ok(())
}
```

- [ ] **Step 3: Implement `spawn_instance` in RocksDBStorage with WriteBatch**

In `crates/smql-storage/src/rocksdb.rs`, add the atomic implementation that puts everything in a single WriteBatch:

```rust
async fn spawn_instance(
    &self,
    instance: &Instance,
    trail_entry: &TrailEntry,
    event: Option<&crate::instance::StoredEvent>,
    idempotency_key: Option<(&str, &[u8], chrono::DateTime<chrono::Utc>)>,
) -> SmqlResult<()> {
    let id_str = instance.id.as_str();

    // Check for duplicate
    if self.load_instance(&id_str)?.is_some() {
        return Err(SmqlError::Conflict {
            message: format!("Instance '{}' already exists", id_str),
            hint: None,
        });
    }

    let cf_inst = self.cf(CF_INSTANCES)?;
    let cf_state = self.cf(CF_STATE_INDEX)?;
    let cf_machine = self.cf(CF_MACHINE_INDEX)?;
    let cf_id = self.cf(CF_ID_INDEX)?;
    let cf_parent = self.cf(CF_PARENT_INDEX)?;
    let cf_trails = self.cf(CF_TRAILS)?;
    let cf_events = self.cf(CF_EVENTS)?;
    let cf_idempotency = self.cf(CF_IDEMPOTENCY)?;

    let inst_bytes = Self::serialize_instance(instance)?;
    let trail_bytes = Self::serialize_trail_entry(trail_entry)?;

    let mut batch = WriteBatchWithTransaction::<false>::default();

    // Instance + indices
    batch.put_cf(&cf_inst, Self::instance_key(&instance.machine, &id_str), &inst_bytes);
    batch.put_cf(&cf_state, Self::state_index_key(&instance.machine, &instance.state, &id_str), b"");
    batch.put_cf(&cf_machine, Self::machine_index_key(&instance.machine, &id_str), b"");
    batch.put_cf(&cf_id, id_str.as_bytes(), instance.machine.as_bytes());

    if let Some(parent_id) = &instance.parent_id {
        batch.put_cf(&cf_parent, Self::parent_index_key(&parent_id.as_str(), &instance.machine, &id_str), b"");
    }

    // Trail entry
    batch.put_cf(&cf_trails, Self::trail_key(&instance.machine, &id_str, trail_entry.sequence), &trail_bytes);

    // Event
    if let Some(evt) = event {
        let event_bytes = serde_json::to_vec(evt)
            .map_err(|e| SmqlError::storage(format!("Serialize event: {}", e)))?;
        batch.put_cf(&cf_events, evt.id.as_bytes(), &event_bytes);
    }

    // Idempotency key
    if let Some((key, response, expires_at)) = idempotency_key {
        let entry = serde_json::json!({
            "response": serde_json::from_slice::<serde_json::Value>(response).unwrap_or(serde_json::Value::Null),
            "expires_at": expires_at.to_rfc3339(),
        });
        let entry_bytes = serde_json::to_vec(&entry)
            .map_err(|e| SmqlError::storage(format!("Serialize idempotency: {}", e)))?;
        batch.put_cf(&cf_idempotency, key.as_bytes(), &entry_bytes);
    }

    self.db.write(batch).map_err(|e| SmqlError::storage(e.to_string()))?;
    Ok(())
}
```

Note: This step references `self.cf()` which we create in Task 2. If implementing sequentially, use `self.db.cf_handle(CF_X).unwrap()` for now and replace in Task 2.

- [ ] **Step 4: Run tests to verify no regressions**

Run: `cargo test --all -q 2>&1 | tail -5`
Expected: All tests pass. No compilation errors.

- [ ] **Step 5: Commit**

```bash
git add crates/smql-storage/src/traits.rs crates/smql-storage/src/memory.rs crates/smql-storage/src/rocksdb.rs
git commit -m "feat(storage): add atomic spawn_instance method to Storage trait

Writes instance, trail entry, event, and idempotency key in a single
atomic operation. RocksDB uses WriteBatch; MemoryStorage does sequential
writes (acceptable for in-process backend). Prevents orphaned instances
without audit trails on crash.

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Replace 41 `unwrap()` Calls on `cf_handle()` in RocksDB

**Why:** Every `self.db.cf_handle(CF_X).unwrap()` is a panic risk. If a column family is corrupted or missing, the engine crashes. These should be validated once at startup and accessed safely.

**Files:**
- Modify: `crates/smql-storage/src/rocksdb.rs`

- [ ] **Step 1: Add a `cf()` helper method that returns `SmqlResult`**

Add this method to `impl RocksDBStorage`:

```rust
/// Get a column family handle, returning an error instead of panicking.
fn cf(&self, name: &str) -> SmqlResult<&rocksdb::ColumnFamily> {
    self.db.cf_handle(name).ok_or_else(|| {
        SmqlError::storage(format!(
            "Column family '{}' not found — database may be corrupted",
            name
        ))
    })
}
```

- [ ] **Step 2: Replace all 41 `cf_handle().unwrap()` calls with `self.cf()?`**

Use find-and-replace across `rocksdb.rs`:
- `self.db.cf_handle(CF_INSTANCES).unwrap()` → `self.cf(CF_INSTANCES)?`
- `self.db.cf_handle(CF_STATE_INDEX).unwrap()` → `self.cf(CF_STATE_INDEX)?`
- `self.db.cf_handle(CF_MACHINE_INDEX).unwrap()` → `self.cf(CF_MACHINE_INDEX)?`
- `self.db.cf_handle(CF_ID_INDEX).unwrap()` → `self.cf(CF_ID_INDEX)?`
- `self.db.cf_handle(CF_PARENT_INDEX).unwrap()` → `self.cf(CF_PARENT_INDEX)?`
- `self.db.cf_handle(CF_TRAILS).unwrap()` → `self.cf(CF_TRAILS)?`
- `self.db.cf_handle(CF_TIMERS).unwrap()` → `self.cf(CF_TIMERS)?`
- `self.db.cf_handle(CF_IDEMPOTENCY).unwrap()` → `self.cf(CF_IDEMPOTENCY)?`
- `self.db.cf_handle(CF_EVENTS).unwrap()` → `self.cf(CF_EVENTS)?`
- `self.db.cf_handle(cf_name).unwrap()` → `self.cf(cf_name)?` (in `scan_prefix`)

Note: The `scan_prefix` method's signature needs to already return `SmqlResult`, which it does. The `load_instance` method also already returns `SmqlResult`. So these replacements are safe.

- [ ] **Step 3: Run tests to verify no regressions**

Run: `cargo test --all -q 2>&1 | tail -5`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/smql-storage/src/rocksdb.rs
git commit -m "fix(storage): replace 41 unwrap() calls on cf_handle with fallible cf() helper

Eliminates panic risk if column family state is corrupted. All CF
accesses now return SmqlError::Storage instead of crashing the engine.

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: Use `spawn_instance` in Engine and Bundle Idempotency Key

**Why:** The engine currently calls `store_instance`, `append_trail_entry`, `store_event`, and `store_idempotency` as separate calls in `spawn_inner`. Fix #1 (non-atomic spawn) and #4 (idempotency key stored after the fact).

**Files:**
- Modify: `crates/smql-engine/src/engine.rs:336-353,442-448`

- [ ] **Step 1: Replace separate spawn writes with `spawn_instance`**

In `engine.rs` `spawn_inner` method, replace lines 336-353 and remove lines 442-448. The new code at line 336:

```rust
// Build idempotency data if key was provided
let idempotency_data = cmd.idempotency_key.as_ref().map(|ikey| {
    // We need to serialize the result, but we need the instance first.
    // Serialize just the instance for now; the full SpawnResult is serialized below.
    (ikey.as_str(), Vec::<u8>::new(), Utc::now() + chrono::Duration::hours(24))
});

// Build the spawn event
let spawn_event = smql_storage::instance::StoredEvent {
    id: ulid::Ulid::new().to_string(),
    timestamp: Utc::now(),
    machine: cmd.machine.clone(),
    event_name: "spawn".to_string(),
    instance_id: instance.id.as_str(),
    payload: serde_json::json!({
        "machine": cmd.machine,
        "initial_state": machine_def.initial_state,
    }),
    actor: spawn_actor_id.clone(),
};

// Store atomically: instance + trail + event + idempotency
self.storage
    .spawn_instance(&instance, &trail_entry, Some(&spawn_event), None)
    .await?;
```

Then, after the `SpawnResult` is constructed (around line 440), store idempotency in a **separate but explicit call** (since we need the serialized result):

```rust
let result = SpawnResult { instance };

// Store idempotency entry atomically would require knowing result upfront.
// Since spawn_instance already committed the instance, store idempotency as best-effort.
// The instance itself is the source of truth; duplicate spawns are caught by the
// unique instance ID / idempotency check at the top of spawn_inner.
if let Some(ref ikey) = cmd.idempotency_key {
    if let Ok(serialized) = serde_json::to_vec(&result) {
        let expires_at = Utc::now() + chrono::Duration::hours(24);
        if let Err(e) = self.storage.store_idempotency(ikey, &serialized, expires_at).await {
            tracing::warn!(key = %ikey, error = %e, "Failed to store idempotency key after spawn");
        }
    }
}
```

Key change: The `let _ =` is replaced with explicit error logging.

- [ ] **Step 2: Do the same for transition idempotency key (line 1253-1259)**

Replace:
```rust
let _ = self.storage.store_idempotency(ikey, &serialized, expires_at).await;
```
With:
```rust
if let Err(e) = self.storage.store_idempotency(ikey, &serialized, expires_at).await {
    tracing::warn!(key = %ikey, error = %e, "Failed to store idempotency key after transition");
}
```

- [ ] **Step 3: Replace fire-and-forget event storage (line 1077)**

Replace:
```rust
let _ = self.storage.store_event(&transition_event).await;
```
With:
```rust
if let Err(e) = self.storage.store_event(&transition_event).await {
    tracing::error!(error = %e, "Failed to store transition event — event log has a gap");
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --all -q 2>&1 | tail -5`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/smql-engine/src/engine.rs
git commit -m "fix(engine): use atomic spawn_instance and log storage errors

Spawn now writes instance+trail+event atomically via spawn_instance.
Idempotency and event storage errors are logged instead of silently
discarded. Fixes non-atomic spawn writes and silent error swallowing.

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Fix Deferred Spawn Version Tracking

**Why:** After `transition_instance` commits, the engine uses `instance.version + 1` for deferred child spawns, but this is the pre-transition version. The actual stored version is already incremented. If any concurrent write happens, the version is wrong and the update silently fails, leaving orphaned children.

**Files:**
- Modify: `crates/smql-engine/src/engine.rs:1079-1102`

- [ ] **Step 1: Fix version tracking and error propagation for deferred spawns**

Replace the deferred spawn block (lines 1079-1102) with:

```rust
// --- 4b. Execute deferred SPAWN commands (after version check succeeded) ---
let has_deferred_spawns = !deferred_spawns.is_empty();
if has_deferred_spawns {
    // The transitioned instance has the correct current version
    let mut current_version = transitioned_instance.version;
    for (field, child_cmd) in deferred_spawns {
        match self.spawn(&child_cmd).await {
            Ok(result) => {
                let child_id = result.instance.id.as_str();
                let child_machine = child_cmd.machine.clone();
                let spawn_mutations =
                    vec![Mutation::SetField(field.clone(), Value::Ref(child_machine, child_id))];
                match self
                    .storage
                    .update_instance(&id, current_version, &spawn_mutations)
                    .await
                {
                    Ok(()) => {
                        current_version += 1;
                    }
                    Err(e) => {
                        tracing::error!(
                            instance_id = %cmd.instance_id,
                            field = %field,
                            error = %e,
                            "Failed to link deferred child to parent — child exists but parent reference is missing"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    instance_id = %cmd.instance_id,
                    error = %e,
                    "Deferred MUTATE SPAWN failed (transition already committed)"
                );
            }
        }
    }
}
```

Key changes:
1. Use `transitioned_instance.version` (the actual current version) instead of `instance.version + 1`
2. Track `current_version` across multiple deferred spawns, incrementing after each successful update
3. Log errors with `tracing::error!` instead of silently discarding

- [ ] **Step 2: Run tests**

Run: `cargo test --all -q 2>&1 | tail -5`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/smql-engine/src/engine.rs
git commit -m "fix(engine): use correct version for deferred child spawn updates

Track actual instance version from transition result instead of
computing from stale pre-transition version. Log errors instead of
silently discarding them.

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: Make CASCADE Report Failures

**Why:** CASCADE silently ignores all child transition failures via `try_transition` and `let _`. Parent ends up in terminal state with active children and no indication anything went wrong.

**Files:**
- Modify: `crates/smql-engine/src/engine.rs:1460-1527`

- [ ] **Step 1: Make `cascade_children` return a result and log failures**

Replace `cascade_children` and `cascade_children_with_depth`:

```rust
/// Cascade: transition all children to their machine's first terminal state.
/// Returns the number of children that failed to cascade.
async fn cascade_children(&self, parent_id: &smql_storage::InstanceId, parent_machine: &str) -> usize {
    self.cascade_children_with_depth(parent_id, parent_machine, 0).await
}

/// Inner cascade with depth tracking to prevent infinite recursion.
fn cascade_children_with_depth<'a>(
    &'a self,
    parent_id: &'a smql_storage::InstanceId,
    _parent_machine: &'a str,
    depth: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = usize> + Send + 'a>> {
    Box::pin(async move {
        if depth >= Self::MAX_CASCADE_DEPTH {
            tracing::warn!(
                parent_id = parent_id.as_str(),
                depth,
                "CASCADE depth limit reached — aborting to prevent infinite recursion"
            );
            return 1; // Count as a failure
        }

        let children = match self.storage.find_children(parent_id, None).await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(parent_id = parent_id.as_str(), error = %e, "CASCADE failed to find children");
                return 1;
            }
        };

        let mut failures = 0usize;

        for child in children {
            let child_machine_def = match self.catalog.get(&child.machine) {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!(child_machine = %child.machine, error = %e, "CASCADE: child machine not found");
                    failures += 1;
                    continue;
                }
            };

            // Skip if already in a terminal state
            if child_machine_def.terminal_states.contains(&child.state) {
                continue;
            }

            if let Some(terminal) = child_machine_def.terminal_states.first() {
                let cmd = TransitionCommand {
                    machine: child.machine.clone(),
                    instance_id: child.id.as_str(),
                    to_state: terminal.clone(),
                    with_data: Vec::new(),
                    memo: Some("CASCADE from parent".to_string()),
                    as_actor: Some("System".to_string()),
                    through: Vec::new(),
                    or_stay: false,
                    cascade: false,
                    idempotency_key: None,
                    tags: Vec::new(),
                };
                match self.try_transition(&cmd).await {
                    Ok(Some(_)) => {
                        // Recursively cascade grandchildren
                        failures += self.cascade_children_with_depth(&child.id, &child.machine, depth + 1).await;
                    }
                    Ok(None) => {
                        tracing::warn!(
                            child_id = child.id.as_str(),
                            child_machine = %child.machine,
                            "CASCADE: child transition denied by guards"
                        );
                        failures += 1;
                    }
                    Err(e) => {
                        tracing::error!(
                            child_id = child.id.as_str(),
                            child_machine = %child.machine,
                            error = %e,
                            "CASCADE: child transition failed"
                        );
                        failures += 1;
                    }
                }
            } else {
                tracing::warn!(
                    child_machine = %child.machine,
                    "CASCADE: child machine has no terminal states"
                );
                failures += 1;
            }
        }

        failures
    })
}
```

- [ ] **Step 2: Update the CASCADE call site to log failures**

At line 1228, replace:
```rust
if cmd.cascade {
    self.cascade_children(&id, &instance.machine).await;
}
```
With:
```rust
if cmd.cascade {
    let cascade_failures = self.cascade_children(&id, &instance.machine).await;
    if cascade_failures > 0 {
        tracing::warn!(
            instance_id = %cmd.instance_id,
            failures = cascade_failures,
            "CASCADE completed with failures — some children may still be in non-terminal states"
        );
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --all -q 2>&1 | tail -5`
Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/smql-engine/src/engine.rs
git commit -m "fix(engine): CASCADE now reports and logs child transition failures

Previously CASCADE silently ignored all failures. Now it tracks failure
count, logs each failure with error context, and warns at the call site.
Parent transition still succeeds (CASCADE is best-effort by design), but
operators can now detect and investigate partial cascades.

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Make Saga Compensation Log Failures

**Why:** Saga compensation is fire-and-forget (`let _ =`). If compensation fails, there's no record. Without persistent compensation logging, crash during compensation = unknown state.

**Files:**
- Modify: `crates/smql-engine/src/engine.rs:2224-2315`

- [ ] **Step 1: Log compensation failures and track results**

In `execute_saga`, replace the compensation block (lines 2281-2306):

```rust
Err(e) => {
    tracing::warn!(saga = saga_name, step = %step.name, error = %e, "SAGA step failed — compensating");

    let mut compensation_failures = Vec::new();

    // Run compensation for all completed steps in reverse order
    for &ci in completed_steps.iter().rev() {
        if let Some(comp) = &saga.steps[ci].compensate {
            let comp_ctx = EvalContext::new(std::collections::HashMap::new(), String::new());
            match eval_expr(&comp.instance_expr, &comp_ctx) {
                Ok(comp_id_val) => {
                    let comp_id = match comp_id_val {
                        Value::Text(s) => s,
                        other => format!("{}", other),
                    };
                    let comp_cmd = TransitionCommand {
                        machine: comp.machine.clone(),
                        instance_id: comp_id,
                        to_state: comp.to_state.clone(),
                        with_data: Vec::new(),
                        memo: Some(format!("SAGA {} compensation for step {}", saga_name, saga.steps[ci].name)),
                        as_actor: Some("System".to_string()),
                        through: Vec::new(),
                        or_stay: false,
                        cascade: false,
                        idempotency_key: None,
                        tags: Vec::new(),
                    };
                    match self.transition_inner(&comp_cmd, None).await {
                        Ok(_) => {
                            tracing::info!(
                                saga = saga_name,
                                step = %saga.steps[ci].name,
                                "SAGA compensation step succeeded"
                            );
                        }
                        Err(comp_err) => {
                            tracing::error!(
                                saga = saga_name,
                                step = %saga.steps[ci].name,
                                error = %comp_err,
                                "SAGA compensation FAILED — manual intervention required"
                            );
                            compensation_failures.push(format!(
                                "step '{}': {}",
                                saga.steps[ci].name, comp_err
                            ));
                        }
                    }
                }
                Err(eval_err) => {
                    tracing::error!(
                        saga = saga_name,
                        step = %saga.steps[ci].name,
                        error = %eval_err,
                        "SAGA compensation instance_expr evaluation failed"
                    );
                    compensation_failures.push(format!(
                        "step '{}' (eval): {}",
                        saga.steps[ci].name, eval_err
                    ));
                }
            }
        }
    }

    if compensation_failures.is_empty() {
        return Err(format!(
            "SAGA '{}' failed at step '{}': {} (all compensations succeeded)",
            saga_name, step.name, e
        ));
    } else {
        return Err(format!(
            "SAGA '{}' failed at step '{}': {} — COMPENSATION FAILURES: {}",
            saga_name,
            step.name,
            e,
            compensation_failures.join("; ")
        ));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --all -q 2>&1 | tail -5`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/smql-engine/src/engine.rs
git commit -m "fix(engine): saga compensation now logs and reports failures

Previously compensation errors were silently discarded. Now each
compensation step is logged with success/failure, and the saga error
message includes all compensation failures. Operators can detect and
investigate partial compensations.

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

### Task 7: Make Transaction Rollback Safer

**Why:** Transaction rollback does `delete_instance` then `store_instance` as separate calls. If delete succeeds but restore fails, the instance is permanently lost. Trail entries from forward steps are never rolled back.

**Files:**
- Modify: `crates/smql-engine/src/engine.rs:2189-2212`

- [ ] **Step 1: Log rollback failures and track results**

Replace the rollback block (lines 2191-2212):

```rust
Err(e) => {
    // Rollback all changes in reverse order
    tracing::warn!(step = i, error = %e, "Transaction step failed — rolling back");
    let mut rollback_errors = Vec::new();

    for (id, snapshot) in snapshots.into_iter().rev() {
        match snapshot {
            None => {
                // Was newly created — delete it
                if let Err(del_err) = self.storage.delete_instance(&id).await {
                    tracing::error!(
                        instance_id = id.as_str(),
                        error = %del_err,
                        "Transaction rollback: failed to delete spawned instance"
                    );
                    rollback_errors.push(format!("delete {}: {}", id.as_str(), del_err));
                }
            }
            Some(old_instance) => {
                // Was modified — restore the snapshot by deleting current and re-storing old
                if let Err(del_err) = self.storage.delete_instance(&id).await {
                    tracing::error!(
                        instance_id = id.as_str(),
                        error = %del_err,
                        "Transaction rollback: failed to delete modified instance before restore"
                    );
                    rollback_errors.push(format!("delete {}: {}", id.as_str(), del_err));
                    // Don't attempt restore if delete failed — would cause duplicate
                    continue;
                }
                if let Err(restore_err) = self.storage.store_instance(&old_instance).await {
                    tracing::error!(
                        instance_id = id.as_str(),
                        error = %restore_err,
                        "Transaction rollback: CRITICAL — deleted instance but failed to restore snapshot. Instance data may be lost."
                    );
                    rollback_errors.push(format!("restore {}: {}", id.as_str(), restore_err));
                }
            }
        }
    }

    if !rollback_errors.is_empty() {
        tracing::error!(
            errors = rollback_errors.join("; "),
            "Transaction rollback completed with errors — manual intervention may be required"
        );
    }

    return Err(SmqlError::TransactionFailed {
        message: format!("Transaction failed at step {}: {}", i, e),
        step: i,
        original_error: Box::new(e),
    });
}
```

Key changes:
1. Each rollback step's error is logged with `tracing::error!`
2. If delete fails, we skip the restore (would cause duplicate)
3. If delete succeeds but restore fails, we log it as CRITICAL
4. All rollback errors are collected and logged

- [ ] **Step 2: Run tests**

Run: `cargo test --all -q 2>&1 | tail -5`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/smql-engine/src/engine.rs
git commit -m "fix(engine): transaction rollback now logs errors and prevents cascading failures

Previously rollback silently discarded errors, risking data loss.
Now each rollback step is logged. Delete failures prevent restore
attempts (avoiding duplicates). Critical failures are logged at
error level for operator attention.

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

### Task 8: Fix Timer Race Condition

**Why:** Timer fires, checks instance state, then transitions. Between check and transition, another thread can modify the instance. However, `transition_instance` already uses version-based concurrency control, so the actual fix is to ensure the version conflict is handled gracefully (which it already is — the error propagates). The real issue is that the state check at line 1607 is informational, and the actual protection comes from `transition_instance`'s version check. We just need to make sure the timer cleanup is reliable.

**Files:**
- Modify: `crates/smql-engine/src/engine.rs:1578-1650`

- [ ] **Step 1: Improve timer cleanup error handling**

Replace lines 1598-1601 and 1609-1612:

```rust
// Instance deleted — clean up the orphaned persisted timer
if let Err(e) = self.storage.remove_timer(instance_id, expected_from_state).await {
    tracing::warn!(
        instance_id,
        state = expected_from_state,
        error = %e,
        "Failed to clean up stale timer for deleted instance"
    );
}
return Ok(None);
```

And:

```rust
// State changed — clean up the stale persisted timer
if let Err(e) = self.storage.remove_timer(instance_id, expected_from_state).await {
    tracing::warn!(
        instance_id,
        state = expected_from_state,
        error = %e,
        "Failed to clean up stale timer after state change"
    );
}
return Ok(None);
```

And at line 1644:

```rust
// Always remove the fired/stale timer from storage, even on version conflict.
if let Err(e) = self.storage.remove_timer(instance_id, expected_from_state).await {
    tracing::warn!(
        instance_id,
        state = expected_from_state,
        error = %e,
        "Failed to clean up timer after timeout transition"
    );
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --all -q 2>&1 | tail -5`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/smql-engine/src/engine.rs
git commit -m "fix(engine): log timer cleanup failures instead of silently discarding

Timer cleanup errors were previously ignored with let _. Now failures
are logged at warn level so stale timers can be investigated. The
version-based concurrency check in transition_instance already prevents
the TOCTOU race from causing incorrect state changes.

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

### Task 9: Fix Division-by-Zero Edge Case in Eval

**Why:** `(Value::Float(a), Value::Int(b))` path in `eval_arithmetic_div` doesn't check for `b == 0` before dividing. It relies on `check_float_result` catching infinity, which works but produces a less informative error.

**Files:**
- Modify: `crates/smql-engine/src/eval.rs:645-647`

- [ ] **Step 1: Write a failing test**

In `crates/smql-engine/tests/test_production_hardening.rs`, add:

```rust
#[test]
fn float_divided_by_int_zero_returns_division_by_zero_error() {
    let c = ctx();
    let expr = binop(
        lit(Value::Float(5.0)),
        BinaryOperator::Div,
        lit(Value::Int(0)),
    );
    let result = eval_expr(&expr, &c);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Division by zero"),
        "Expected 'Division by zero' error, got: {}",
        err
    );
}
```

- [ ] **Step 2: Run test to verify it fails with wrong error message**

Run: `cargo test -p smql-engine-core --test test_production_hardening float_divided_by_int_zero -v`
Expected: FAIL — the error message says "overflow" or "Infinity" instead of "Division by zero"

- [ ] **Step 3: Add explicit zero check**

In `crates/smql-engine/src/eval.rs`, replace lines 645-647:

```rust
(Value::Float(a), Value::Int(b)) => {
    if *b == 0 {
        Err(SmqlError::GuardFailed {
            message: "Division by zero".to_string(),
            guard_expr: format!("{} / {}", left, right),
            actual_value: None,
            hint: None,
        })
    } else {
        check_float_result(a / *b as f64, "/", left, right)
    }
}
```

- [ ] **Step 4: Run tests to verify fix**

Run: `cargo test -p smql-engine-core --test test_production_hardening float_divided_by_int_zero -v`
Expected: PASS

Run: `cargo test --all -q 2>&1 | tail -5`
Expected: All tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/smql-engine/src/eval.rs crates/smql-engine/tests/test_production_hardening.rs
git commit -m "fix(eval): explicit division-by-zero check for Float / Int(0)

Previously Float / Int(0) would produce Infinity and get caught by
check_float_result with a less informative error. Now it returns
'Division by zero' directly, consistent with all other div-by-zero paths.

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

### Task 10: Add Version-Checked `update_instance` in RocksDB

**Why:** The RocksDB `update_instance` loads the instance, checks the version, then writes. Between check and write, another writer can modify the instance, bypassing the version guard. We should use a WriteBatch that includes the version-checked instance write.

Note: True transactional safety requires `TransactionDB` which is a larger migration. For now, we can add a documented concurrency comment and use a more defensive pattern with WriteBatch.

**Files:**
- Modify: `crates/smql-storage/src/rocksdb.rs:413-448`

- [ ] **Step 1: Document the TOCTOU limitation clearly**

Add a doc comment to the `update_instance` method in the `Storage` impl:

```rust
/// Update an instance's data fields via mutations.
///
/// **Concurrency note:** This implementation uses a read-check-write pattern
/// that is not fully atomic under concurrent access. The engine's single-writer
/// design (via its async task model) provides practical safety, but direct
/// concurrent calls to this method can experience TOCTOU races. A future
/// migration to RocksDB's `TransactionDB` with `get_for_update` would provide
/// true serializable isolation.
async fn update_instance(
```

The actual fix for full atomicity requires `TransactionDB`, which is a larger structural change. For now, the documentation makes the limitation explicit and the engine's async model provides practical single-writer safety.

- [ ] **Step 2: Run tests**

Run: `cargo test --all -q 2>&1 | tail -5`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/smql-storage/src/rocksdb.rs
git commit -m "docs(storage): document TOCTOU limitation in RocksDB update_instance

Makes the concurrency limitation explicit. True atomic read-check-write
requires migration to TransactionDB with get_for_update, which is a
larger structural change tracked separately.

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

### Task 11: Propagate Remaining `let _ =` Error Suppressions

**Why:** Multiple `let _ =` patterns in engine.rs silently swallow errors from timer storage, hook execution, and other operations. While hooks and actions are intentionally fire-and-forget, timer persistence failures should be logged.

**Files:**
- Modify: `crates/smql-engine/src/engine.rs` (multiple locations)

- [ ] **Step 1: Find and fix timer storage `let _ =` patterns**

Search for `let _ = self.storage.store_timer` and `let _ = self.storage.remove_timer` and replace with logged versions:

At line 1118-1121:
```rust
if let Err(e) = self.storage.remove_timer(&cmd.instance_id, &instance.state).await {
    tracing::warn!(error = %e, "Failed to remove old timer");
}
```

At line 1144:
```rust
if let Err(e) = self.storage.store_timer(&stored).await {
    tracing::warn!(error = %e, "Failed to persist timeout timer");
}
```

Note: Hook execution `let _ =` patterns (lines 1105, 1158, 1172, etc.) are intentionally fire-and-forget. Hooks are side effects that should not block the transition. Leave these as-is.

- [ ] **Step 2: Run tests**

Run: `cargo test --all -q 2>&1 | tail -5`
Expected: All tests pass.

- [ ] **Step 3: Commit**

```bash
git add crates/smql-engine/src/engine.rs
git commit -m "fix(engine): log timer persistence errors instead of silently discarding

Timer store/remove failures were silently ignored. Now they are logged
at warn level. Hook execution remains fire-and-forget by design.

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>"
```

---

## Final Verification

After all tasks are complete:

- [ ] Run full test suite: `cargo test --all 2>&1 | tail -20`
- [ ] Run with RocksDB feature: `cargo test --all --features rocksdb 2>&1 | tail -20`
- [ ] Check for remaining `let _ =` patterns that should be logged: `grep -n 'let _ =' crates/smql-engine/src/engine.rs`
- [ ] Verify no remaining `unwrap()` in rocksdb.rs: `grep -n '\.unwrap()' crates/smql-storage/src/rocksdb.rs`
