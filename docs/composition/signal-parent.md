# Signal Parent

`SIGNAL PARENT TO` allows a child instance to trigger a transition on its parent. This is the upward communication channel in a composition hierarchy -- children can tell the parent "something happened, move to this state."

## Syntax

Inside a child machine's transition block:

```sql
source -> target {
  SIGNAL PARENT TO parent_state
}
```

## How It Works

When a child transition includes `SIGNAL PARENT TO`, the engine performs these steps in order:

1. The child's transition completes (guards pass, mutations apply, state changes)
2. The engine looks up the child's `parent_id`
3. The engine attempts to transition the parent to the specified state
4. If the parent's guards for that transition pass, the parent moves to the new state
5. If the parent's guards fail, the child's transition still succeeds -- the signal is best-effort on the parent side

## Example: Shipment Delivers an Order

A Shipment signals its parent Order when delivery is confirmed:

```sql
DEFINE MACHINE Shipment (
  PARENT : Order
  DATA {
    tracking : TEXT -> OPTIONAL
    carrier  : ENUM(fedex, ups, dhl, usps) -> OPTIONAL
  }
  STATES { created, dispatched, in_transit, delivered, lost }
  INITIAL STATE created
  TERMINAL STATES { delivered, lost }
  TRANSITIONS {
    created -> dispatched {
      GUARD  : tracking IS SET
      GUARD  : carrier IS SET
      ACTION : NOTIFY(PARENT.customer, "order.shipped")
    }
    dispatched -> in_transit {}
    in_transit -> delivered {
      SIGNAL PARENT TO delivered
    }
    in_transit -> lost {
      ACTION : NOTIFY(PARENT.customer, "shipment.lost")
    }
  }
)
```

When the Shipment transitions from `in_transit` to `delivered`, the engine automatically transitions the parent Order to `delivered`. The Order must have a valid transition path to `delivered` from its current state (e.g., `shipped -> delivered`).

## The Full Flow

Here is the sequence for the Order-Shipment example:

```
1. Order:    paid -> fulfilled     (spawns Shipment, guards on ALL items confirmed)
2. Shipment: created -> dispatched (tracking and carrier set)
3. Order:    fulfilled -> shipped  (guards on shipment.STATE IS dispatched)
4. Shipment: dispatched -> in_transit
5. Shipment: in_transit -> delivered  --> SIGNAL PARENT TO delivered
6. Order:    shipped -> delivered   (triggered by signal)
```

Steps 5 and 6 happen atomically from the child's transition. The parent transition is triggered as part of the same operation.

## Signal and Guards

The parent transition triggered by a signal still evaluates the parent's guards. If the parent defines guards on the target transition:

```sql
-- In the Order machine
shipped -> delivered {
  GUARD : ALL(items, STATE IS confirmed)
}
```

The signal from the Shipment will attempt this transition. If the guard fails (perhaps a LineItem is not confirmed), the parent stays in its current state. The child transition still completes successfully.

## Signal Does Not Carry Data

`SIGNAL PARENT TO` only specifies the target state. It does not pass data or an actor. The parent transition executes without an actor and without WITH data. Keep this in mind when designing guards -- the parent transition triggered by a signal should not require actor-based guards.

## Multi-Level Signals

In a multi-level hierarchy (e.g., Pipeline > Stage > Job), a Job can signal its parent Stage, and the Stage can in turn signal its parent Pipeline. Each signal only reaches one level up -- there is no transitive signaling across levels. If you need a Job completion to affect the Pipeline, the Stage must have its own `SIGNAL PARENT TO`:

```sql
-- Job signals Stage
DEFINE MACHINE Job (
  PARENT : Stage
  TRANSITIONS {
    running -> completed {
      SIGNAL PARENT TO completed
    }
  }
)

-- Stage signals Pipeline
DEFINE MACHINE Stage (
  PARENT : Pipeline
  TRANSITIONS {
    running -> completed {
      GUARD : ALL(jobs, STATE IS completed)
      SIGNAL PARENT TO completed
    }
  }
)
```

When the last Job completes, it signals the Stage. The Stage's guard checks that all Jobs are completed, and if so, the Stage transitions and signals the Pipeline.

::: tip
Signals propagate one level at a time. Design each level to check its own children and decide whether to signal upward. This keeps the logic local and predictable.
:::

::: warning
If the parent has no valid transition from its current state to the signaled state, the signal is silently ignored. The child's transition still succeeds. Always verify that the parent machine defines the expected transition path.
:::
