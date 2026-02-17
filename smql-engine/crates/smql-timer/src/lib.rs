// SMQL Timer — Timeout and scheduled transition manager

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use smql_ast::value::SmqlDuration;
use std::collections::{BTreeMap, HashMap};
use std::sync::RwLock;

/// A registered timer that fires a transition at a deadline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerEntry {
    pub instance_id: String,
    pub machine: String,
    pub from_state: String,
    pub target_state: String,
    pub deadline: DateTime<Utc>,
    pub registered_at: DateTime<Utc>,
}

/// Manages timeout timers for state machine instances.
///
/// Uses a BTreeMap keyed by deadline for efficient "what fires next?" lookups,
/// and a HashMap keyed by (instance_id:state) for O(1) cancellation.
pub struct TimerManager {
    /// Timers indexed by deadline for efficient expiry checks.
    timers_by_deadline: RwLock<BTreeMap<DateTime<Utc>, Vec<TimerEntry>>>,
    /// Maps (instance_id:state) -> deadline for fast cancellation.
    timers_by_key: RwLock<HashMap<String, DateTime<Utc>>>,
}

impl TimerManager {
    pub fn new() -> Self {
        Self {
            timers_by_deadline: RwLock::new(BTreeMap::new()),
            timers_by_key: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new timeout timer.
    ///
    /// Cancels any existing timer for this instance+state before registering.
    pub fn register(
        &self,
        instance_id: &str,
        machine: &str,
        state: &str,
        duration: &SmqlDuration,
        target_state: &str,
    ) {
        let now = Utc::now();
        let deadline = now + TimeDelta::seconds(duration.seconds as i64);
        self.register_with_deadline(instance_id, machine, state, target_state, deadline, now);
    }

    /// Register a timer with an explicit deadline (useful for testing and persistence).
    pub fn register_with_deadline(
        &self,
        instance_id: &str,
        machine: &str,
        state: &str,
        target_state: &str,
        deadline: DateTime<Utc>,
        registered_at: DateTime<Utc>,
    ) {
        let key = timer_key(instance_id, state);

        // Cancel any existing timer for this instance+state
        self.cancel(instance_id, state);

        let entry = TimerEntry {
            instance_id: instance_id.to_string(),
            machine: machine.to_string(),
            from_state: state.to_string(),
            target_state: target_state.to_string(),
            deadline,
            registered_at,
        };

        let mut by_deadline = self.timers_by_deadline.write().unwrap();
        let mut by_key = self.timers_by_key.write().unwrap();

        by_deadline.entry(deadline).or_default().push(entry);
        by_key.insert(key, deadline);
    }

    /// Cancel a timer for a specific instance and state.
    pub fn cancel(&self, instance_id: &str, state: &str) {
        let key = timer_key(instance_id, state);
        let mut by_key = self.timers_by_key.write().unwrap();
        let mut by_deadline = self.timers_by_deadline.write().unwrap();

        if let Some(deadline) = by_key.remove(&key) {
            if let Some(entries) = by_deadline.get_mut(&deadline) {
                entries.retain(|e| !(e.instance_id == instance_id && e.from_state == state));
                if entries.is_empty() {
                    by_deadline.remove(&deadline);
                }
            }
        }
    }

    /// Cancel all timers for a specific instance.
    pub fn cancel_all(&self, instance_id: &str) {
        let mut by_key = self.timers_by_key.write().unwrap();
        let mut by_deadline = self.timers_by_deadline.write().unwrap();

        let prefix = format!("{}:", instance_id);
        let keys_to_remove: Vec<String> = by_key
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();

        for key in &keys_to_remove {
            if let Some(deadline) = by_key.remove(key) {
                if let Some(entries) = by_deadline.get_mut(&deadline) {
                    entries.retain(|e| e.instance_id != instance_id);
                    if entries.is_empty() {
                        by_deadline.remove(&deadline);
                    }
                }
            }
        }
    }

    /// Drain all expired timers and return them.
    /// Removes them from the manager.
    pub fn drain_expired(&self) -> Vec<TimerEntry> {
        let now = Utc::now();
        let mut by_deadline = self.timers_by_deadline.write().unwrap();
        let mut by_key = self.timers_by_key.write().unwrap();

        let mut expired = Vec::new();

        // Collect all deadline keys that are <= now
        let expired_keys: Vec<DateTime<Utc>> = by_deadline.range(..=now).map(|(k, _)| *k).collect();

        for deadline in expired_keys {
            if let Some(entries) = by_deadline.remove(&deadline) {
                for entry in entries {
                    let key = timer_key(&entry.instance_id, &entry.from_state);
                    by_key.remove(&key);
                    expired.push(entry);
                }
            }
        }

        expired
    }

    /// Get the timeout remaining for an instance in its current state.
    /// Returns None if no timer is registered.
    pub fn timeout_remaining(&self, instance_id: &str, state: &str) -> Option<TimeDelta> {
        let by_key = self.timers_by_key.read().unwrap();
        let key = timer_key(instance_id, state);

        by_key.get(&key).map(|deadline| {
            let now = Utc::now();
            *deadline - now
        })
    }

    /// Check if any timers are registered.
    pub fn has_timers(&self) -> bool {
        let by_key = self.timers_by_key.read().unwrap();
        !by_key.is_empty()
    }

    /// Get the count of registered timers.
    pub fn timer_count(&self) -> usize {
        let by_key = self.timers_by_key.read().unwrap();
        by_key.len()
    }

    /// Get the next deadline (useful for adaptive sleeping).
    pub fn next_deadline(&self) -> Option<DateTime<Utc>> {
        let by_deadline = self.timers_by_deadline.read().unwrap();
        by_deadline.keys().next().copied()
    }

    /// Get the timer entry for a specific instance and state.
    pub fn get_timer(&self, instance_id: &str, state: &str) -> Option<TimerEntry> {
        let by_key = self.timers_by_key.read().unwrap();
        let by_deadline = self.timers_by_deadline.read().unwrap();
        let key = timer_key(instance_id, state);

        if let Some(deadline) = by_key.get(&key) {
            if let Some(entries) = by_deadline.get(deadline) {
                return entries
                    .iter()
                    .find(|e| e.instance_id == instance_id && e.from_state == state)
                    .cloned();
            }
        }
        None
    }
}

impl Default for TimerManager {
    fn default() -> Self {
        Self::new()
    }
}

fn timer_key(instance_id: &str, state: &str) -> String {
    format!("{}:{}", instance_id, state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    #[test]
    fn register_and_count() {
        let tm = TimerManager::new();
        let dur = SmqlDuration::from_hours(24);
        tm.register("inst_1", "Ticket", "waiting", &dur, "resolved");
        assert_eq!(tm.timer_count(), 1);
        assert!(tm.has_timers());
    }

    #[test]
    fn cancel_timer() {
        let tm = TimerManager::new();
        let dur = SmqlDuration::from_hours(24);
        tm.register("inst_1", "Ticket", "waiting", &dur, "resolved");
        assert_eq!(tm.timer_count(), 1);

        tm.cancel("inst_1", "waiting");
        assert_eq!(tm.timer_count(), 0);
        assert!(!tm.has_timers());
    }

    #[test]
    fn cancel_nonexistent_is_noop() {
        let tm = TimerManager::new();
        tm.cancel("inst_1", "waiting"); // should not panic
        assert_eq!(tm.timer_count(), 0);
    }

    #[test]
    fn cancel_all_for_instance() {
        let tm = TimerManager::new();
        let dur = SmqlDuration::from_hours(24);
        tm.register("inst_1", "Ticket", "waiting", &dur, "resolved");
        tm.register("inst_1", "Ticket", "open", &dur, "triaged");
        tm.register("inst_2", "Ticket", "waiting", &dur, "resolved");
        assert_eq!(tm.timer_count(), 3);

        tm.cancel_all("inst_1");
        assert_eq!(tm.timer_count(), 1);
    }

    #[test]
    fn drain_expired_returns_past_timers() {
        let tm = TimerManager::new();
        let now = Utc::now();

        // Register a timer that already expired (deadline in the past)
        tm.register_with_deadline(
            "inst_1",
            "Ticket",
            "waiting",
            "resolved",
            now - TimeDelta::seconds(10),
            now - TimeDelta::seconds(100),
        );

        // Register a timer in the future
        tm.register_with_deadline(
            "inst_2",
            "Ticket",
            "waiting",
            "resolved",
            now + TimeDelta::hours(1),
            now,
        );

        let expired = tm.drain_expired();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].instance_id, "inst_1");
        assert_eq!(tm.timer_count(), 1); // inst_2 still active
    }

    #[test]
    fn drain_expired_empty_when_none_expired() {
        let tm = TimerManager::new();
        let now = Utc::now();
        tm.register_with_deadline(
            "inst_1",
            "Ticket",
            "waiting",
            "resolved",
            now + TimeDelta::hours(1),
            now,
        );

        let expired = tm.drain_expired();
        assert!(expired.is_empty());
        assert_eq!(tm.timer_count(), 1);
    }

    #[test]
    fn timeout_remaining_returns_positive_for_future_timer() {
        let tm = TimerManager::new();
        let dur = SmqlDuration::from_hours(24);
        tm.register("inst_1", "Ticket", "waiting", &dur, "resolved");

        let remaining = tm.timeout_remaining("inst_1", "waiting");
        assert!(remaining.is_some());
        let r = remaining.unwrap();
        // Should be close to 24h (allowing a few seconds of test execution)
        assert!(r.num_seconds() > 86000);
    }

    #[test]
    fn timeout_remaining_none_for_unknown() {
        let tm = TimerManager::new();
        assert!(tm.timeout_remaining("inst_1", "waiting").is_none());
    }

    #[test]
    fn re_register_replaces_old_timer() {
        let tm = TimerManager::new();
        let dur1 = SmqlDuration::from_hours(24);
        let dur2 = SmqlDuration::from_hours(48);

        tm.register("inst_1", "Ticket", "waiting", &dur1, "resolved");
        assert_eq!(tm.timer_count(), 1);

        // Re-register with different duration — should replace
        tm.register("inst_1", "Ticket", "waiting", &dur2, "closed");
        assert_eq!(tm.timer_count(), 1);

        let entry = tm.get_timer("inst_1", "waiting").unwrap();
        assert_eq!(entry.target_state, "closed");
    }

    #[test]
    fn next_deadline_returns_earliest() {
        let tm = TimerManager::new();
        let now = Utc::now();

        tm.register_with_deadline(
            "inst_2",
            "Ticket",
            "waiting",
            "resolved",
            now + TimeDelta::hours(2),
            now,
        );
        tm.register_with_deadline(
            "inst_1",
            "Ticket",
            "open",
            "triaged",
            now + TimeDelta::hours(1),
            now,
        );

        let next = tm.next_deadline().unwrap();
        assert!(next < now + TimeDelta::hours(2));
    }

    #[test]
    fn get_timer_returns_entry() {
        let tm = TimerManager::new();
        let dur = SmqlDuration::from_hours(72);
        tm.register("inst_1", "Ticket", "waiting_on_customer", &dur, "resolved");

        let entry = tm.get_timer("inst_1", "waiting_on_customer").unwrap();
        assert_eq!(entry.machine, "Ticket");
        assert_eq!(entry.from_state, "waiting_on_customer");
        assert_eq!(entry.target_state, "resolved");
    }

    #[test]
    fn get_timer_returns_none_after_cancel() {
        let tm = TimerManager::new();
        let dur = SmqlDuration::from_hours(24);
        tm.register("inst_1", "Ticket", "waiting", &dur, "resolved");
        tm.cancel("inst_1", "waiting");
        assert!(tm.get_timer("inst_1", "waiting").is_none());
    }

    #[test]
    fn multiple_timers_same_deadline() {
        let tm = TimerManager::new();
        let now = Utc::now();
        let deadline = now + TimeDelta::hours(1);

        tm.register_with_deadline("inst_1", "Ticket", "waiting", "resolved", deadline, now);
        tm.register_with_deadline("inst_2", "Ticket", "waiting", "resolved", deadline, now);
        assert_eq!(tm.timer_count(), 2);

        // Cancel one, the other should remain
        tm.cancel("inst_1", "waiting");
        assert_eq!(tm.timer_count(), 1);
        assert!(tm.get_timer("inst_2", "waiting").is_some());
    }

    #[test]
    fn drain_multiple_expired() {
        let tm = TimerManager::new();
        let now = Utc::now();

        for i in 0..5 {
            tm.register_with_deadline(
                &format!("inst_{}", i),
                "Ticket",
                "waiting",
                "resolved",
                now - TimeDelta::seconds(i as i64 + 1),
                now - TimeDelta::seconds(100),
            );
        }

        let expired = tm.drain_expired();
        assert_eq!(expired.len(), 5);
        assert_eq!(tm.timer_count(), 0);
    }

    #[test]
    fn timer_manager_default() {
        let tm = TimerManager::default();
        assert_eq!(tm.timer_count(), 0);
        assert!(!tm.has_timers());
    }

    #[test]
    fn next_deadline_returns_none_when_empty() {
        let tm = TimerManager::new();
        assert!(tm.next_deadline().is_none());
    }

    #[test]
    fn get_timer_returns_none_for_nonexistent() {
        let tm = TimerManager::new();
        assert!(tm.get_timer("inst_1", "waiting").is_none());
    }

    #[test]
    fn cancel_all_no_matching_timers_is_noop() {
        let tm = TimerManager::new();
        let dur = SmqlDuration::from_hours(24);
        tm.register("inst_1", "Ticket", "waiting", &dur, "resolved");
        // Cancel all for a different instance
        tm.cancel_all("inst_2");
        assert_eq!(tm.timer_count(), 1);
    }

    #[test]
    fn drain_expired_empty_manager() {
        let tm = TimerManager::new();
        let expired = tm.drain_expired();
        assert!(expired.is_empty());
    }
}
