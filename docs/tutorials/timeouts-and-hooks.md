# Tutorial 3: Timeouts & Hooks

In the [previous tutorial](./adding-data-and-guards), you added data fields and guards that control who can do what. But real workflows also involve time — "if nobody responds within 48 hours, escalate" — and side effects — "notify the team when a review is approved."

In this tutorial, you'll add both.

## What You'll Build

A `ReviewRequest` machine for code reviews. It tracks reviews through their lifecycle and adds:
- **Timeouts** that auto-transition when things take too long
- **Hooks** that emit events at key lifecycle moments
- **WebSocket** event streaming to monitor activity in real time

```
pending ──→ in_review ──→ approved ──→ merged
                │              ↑
                ↓              │
            changes_requested ─┘
                │
                ↓ (timeout: 7d)
            stale
```

## Step 1: Define the Machine

```sql
DEFINE MACHINE ReviewRequest (

  DATA {
    pr_url      : TEXT -> REQUIRED
    author      : TEXT -> REQUIRED
    reviewer    : TEXT -> OPTIONAL
    comments    : TEXT -> OPTIONAL
  }

  STATES { pending, in_review, changes_requested, approved, stale, merged }
  INITIAL STATE pending
  TERMINAL STATES { merged, stale }

  TRANSITIONS {
    pending -> in_review {
      GUARD : reviewer IS SET
    }

    in_review -> approved {
      GUARD : ACTOR.id == reviewer
    }

    in_review -> changes_requested {
      GUARD  : ACTOR.id == reviewer
      GUARD  : comments IS SET
      TIMEOUT: 7d -> stale
    }

    changes_requested -> in_review {}

    approved -> merged {
      GUARD : ACTOR.id == author
    }
  }

  HOOKS {
    ON SPAWN {
      EMIT("review.created")
    }

    AFTER EACH TRANSITION {
      EMIT("review.state_changed")
    }

    ON ENTER approved {
      EMIT("review.approved")
    }

    ON ENTER stale {
      EMIT("review.gone_stale")
    }
  }
)
```

## Step 2: Understanding Timeouts

The `changes_requested` transition has a timeout:

```sql
in_review -> changes_requested {
  GUARD  : ACTOR.id == reviewer
  GUARD  : comments IS SET
  TIMEOUT: 7d -> stale
}
```

Here's how it works:

1. When an instance **enters** `changes_requested`, SMQL registers a timer for 7 days
2. If the instance is still in `changes_requested` after 7 days, it automatically transitions to `stale`
3. If the instance **leaves** `changes_requested` before the timer fires (e.g., the author pushes fixes and it goes back to `in_review`), the timer is **canceled**

Key properties of timeouts:

| Property | Behavior |
|----------|----------|
| **Actor** | System actor (not a human) |
| **Guards** | Bypassed — the timeout transition ignores all guards |
| **Cancel** | Automatic when the instance leaves the source state |
| **Trail** | Recorded as a normal transition with actor "System" |

### Duration Formats

SMQL supports natural duration syntax:

```sql
TIMEOUT: 30s -> state       -- seconds
TIMEOUT: 5m -> state        -- minutes
TIMEOUT: 2h -> state        -- hours
TIMEOUT: 7d -> state        -- days
TIMEOUT: 1h 30m -> state    -- combined
```

## Step 3: Walk Through the Lifecycle

### Spawn and Assign

```bash
> SPAWN ReviewRequest { pr_url: "https://github.com/acme/app/pull/42", author: "alice" }
```

The `ON SPAWN` hook fires, emitting a `review.created` event.

```bash
> TRANSITION "<id>" TO in_review WITH { reviewer: "bob" }
```

The `AFTER EACH TRANSITION` hook emits `review.state_changed`.

### Request Changes (Starts the Timer)

```bash
> TRANSITION "<id>" TO changes_requested WITH { comments: "Please add tests" } AS "bob"
```

Two things happen:
1. The transition succeeds (both guards pass)
2. A 7-day timer is registered — if the author doesn't respond, the review goes `stale`

### Author Fixes and Resubmits (Cancels the Timer)

```bash
> TRANSITION "<id>" TO in_review
```

When the instance leaves `changes_requested`, the 7-day timer is **automatically canceled**. No stale transition will happen.

### Approve and Merge

```bash
> TRANSITION "<id>" TO approved AS "bob"
```

The `ON ENTER approved` hook fires, emitting `review.approved`.

```bash
> TRANSITION "<id>" TO merged AS "alice"
```

The instance is now in the terminal state `merged`. No further transitions are possible.

## Step 4: Understanding Hooks

Hooks are lifecycle callbacks defined in the `HOOKS` block. They fire automatically at specific moments.

### Hook Types

| Hook | When It Fires | Blocking? |
|------|---------------|-----------|
| `ON SPAWN` | After a new instance is created | No |
| `BEFORE EACH TRANSITION` | Before any transition proceeds | **Yes** — can reject |
| `AFTER EACH TRANSITION` | After any transition completes | No |
| `ON ENTER state` | After entering a specific state | No |
| `ON EXIT state` | Before leaving a specific state | No |

### EMIT — Firing Events

The most common hook action is `EMIT`, which publishes an event to the EventBus:

```sql
HOOKS {
  ON SPAWN {
    EMIT("review.created")
  }
}
```

Events are available to:
- **WebSocket subscribers** — for real-time monitoring
- **Other hook listeners** — for cross-machine coordination

### BEFORE Hooks (Rejecting Transitions)

`BEFORE EACH TRANSITION` is the only hook type that can **reject** a transition. If it fails, the transition is treated as a guard failure:

```sql
HOOKS {
  BEFORE EACH TRANSITION {
    EMIT("review.transition_attempt")
  }
}
```

::: warning
Use `BEFORE` hooks sparingly. Since they can reject transitions, they add complexity. Prefer guards for access control and data validation.
:::

## Step 5: Monitor Events with WebSocket

SMQL streams events over WebSocket so you can monitor activity in real time. Connect to the subscription endpoint:

```bash
# In a separate terminal — connect to the WebSocket
websocat ws://localhost:4200/subscribe?machine=ReviewRequest
```

Now, in your main terminal, spawn and transition instances. You'll see events appear in the WebSocket terminal:

```json
{"event":"review.created","machine":"ReviewRequest","instance_id":"01JM..."}
{"event":"review.state_changed","machine":"ReviewRequest","instance_id":"01JM..."}
{"event":"review.approved","machine":"ReviewRequest","instance_id":"01JM..."}
```

### Subscribing from the SDK

```rust
use smql_sdk::SmqlClient;

let client = SmqlClient::new("http://localhost:4200")?;
let mut sub = client.subscribe(Some("ReviewRequest")).await?;

loop {
    if let Ok(event) = sub.next_event().await {
        println!("{}: {} ({})", event.event, event.instance_id, event.machine);
    }
}
```

## Step 6: Timeout in Action

Let's see what happens when the timer fires. In a real system, you'd wait 7 days. For testing, use a shorter timeout:

```sql
-- Hypothetical test setup with a short timeout
DEFINE MACHINE QuickReview (
  DATA { title: TEXT -> REQUIRED }
  STATES { open, waiting, expired }
  INITIAL STATE open
  TERMINAL STATES { expired }
  TRANSITIONS {
    open -> waiting {
      TIMEOUT: 5s -> expired
    }
  }
)
```

```bash
> SPAWN QuickReview { title: "Test" }
> TRANSITION "<id>" TO waiting
```

Wait 5 seconds, then:

```bash
> GET QuickReview "<id>"
```

```json
{
  "success": true,
  "result": {
    "state": "expired",
    "trail_length": 3
  }
}
```

The system automatically transitioned from `waiting` to `expired` after 5 seconds. The trail shows:

```bash
> TRAIL OF QuickReview "<id>"
```

```json
{
  "entries": [
    { "sequence": 0, "from_state": "", "to_state": "open" },
    { "sequence": 1, "from_state": "open", "to_state": "waiting" },
    { "sequence": 2, "from_state": "waiting", "to_state": "expired", "actor": "System" }
  ]
}
```

The timeout transition has actor `"System"` — it wasn't triggered by a human.

## What You Learned

| Concept | Summary |
|---------|---------|
| `TIMEOUT` | Automatic transition after a duration if the instance is still in the source state |
| Cancel-on-exit | Timers are canceled when the instance leaves the source state |
| System actor | Timeouts are performed by the system, bypassing guards |
| `HOOKS` block | Lifecycle callbacks that fire at specific moments |
| `ON SPAWN` | Hook that fires when a new instance is created |
| `AFTER EACH TRANSITION` | Hook that fires after every successful transition |
| `ON ENTER state` | Hook that fires when entering a specific state |
| `BEFORE EACH TRANSITION` | The only hook that can reject a transition |
| `EMIT` | Publishes an event to the EventBus for WebSocket and listeners |

## Next Step

Your machine is now time-aware and event-driven. But what about machines that contain other machines — like an order with line items? In the [next tutorial](./composition-patterns), you'll build parent-child machine hierarchies with composition.
