# Tutorial 1: Your First Machine

In this tutorial, you'll define a simple state machine, create instances, transition between states, and inspect the results. By the end, you'll understand the core SMQL workflow.

## What You'll Build

A `TrafficLight` machine with three states: `red`, `green`, and `yellow`. Traffic lights cycle through states in order, and `red` is where every light starts.

```
red ──→ green ──→ yellow ──→ red (cycle)
```

## Start the REPL

Open a terminal and start the interactive REPL with in-memory storage:

```bash
smql repl
```

You'll see a prompt where you can type SMQL statements directly.

## Step 1: Define the Machine

Every state machine starts with `DEFINE MACHINE`. You declare the states, which state is initial, which states are terminal (end states), and the allowed transitions.

::: code-group
```bash [REPL]
> DEFINE MACHINE TrafficLight (
    STATES { red, green, yellow }
    INITIAL STATE red
    TERMINAL STATES {}
    TRANSITIONS {
      red -> green {}
      green -> yellow {}
      yellow -> red {}
    }
  )
```

```bash [curl]
curl -X POST http://localhost:4200/execute \
  -H 'Content-Type: application/json' \
  -d '{
    "smql": "DEFINE MACHINE TrafficLight ( STATES { red, green, yellow } INITIAL STATE red TERMINAL STATES {} TRANSITIONS { red -> green {} green -> yellow {} yellow -> red {} } )"
  }'
```
:::

```json
{
  "success": true,
  "result": { "action": "machine_defined" }
}
```

Let's break down each part:

- **STATES** — the set of all possible states this machine can be in
- **INITIAL STATE** — every new instance starts here
- **TERMINAL STATES** — end states where no further transitions are possible (empty here because traffic lights cycle forever)
- **TRANSITIONS** — the allowed state changes, written as `from -> to {}`

::: tip
The `{}` after each transition is the transition body. It's empty here, but later you'll add guards, actions, and mutations inside it.
:::

## Step 2: Spawn an Instance

A machine is a blueprint. To create an actual traffic light, you **spawn** an instance:

::: code-group
```bash [REPL]
> SPAWN TrafficLight {}
```

```bash [curl]
curl -X POST http://localhost:4200/execute \
  -H 'Content-Type: application/json' \
  -d '{"smql": "SPAWN TrafficLight {}"}'
```
:::

```json
{
  "success": true,
  "result": {
    "id": "01J5X7K2P3Q4R5S6T7U8V9W0XY",
    "machine": "TrafficLight",
    "state": "red",
    "data": {},
    "created_at": "2026-02-16T10:00:00Z",
    "updated_at": "2026-02-16T10:00:00Z",
    "state_entered_at": "2026-02-16T10:00:00Z",
    "trail_length": 1,
    "version": 1
  }
}
```

Key observations:

- The instance got a unique **ULID** identifier (26 characters, time-sortable)
- It starts in the **red** state, as declared by `INITIAL STATE`
- `trail_length: 1` means one event has been recorded (the spawn)
- `version: 1` tracks optimistic concurrency

::: info
Save the `id` from the response — you'll need it for transitions and queries. In the examples below, we'll use `"01J5X7K2P3Q4R5S6T7U8V9W0XY"` as a placeholder.
:::

## Step 3: Transition to the Next State

Move the traffic light from `red` to `green`:

::: code-group
```bash [REPL]
> TRANSITION "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO green
```

```bash [curl]
curl -X POST http://localhost:4200/execute \
  -H 'Content-Type: application/json' \
  -d '{
    "smql": "TRANSITION \"01J5X7K2P3Q4R5S6T7U8V9W0XY\" TO green"
  }'
```
:::

```json
{
  "success": true,
  "result": {
    "from_state": "red",
    "to_state": "green",
    "instance": {
      "id": "01J5X7K2P3Q4R5S6T7U8V9W0XY",
      "state": "green",
      "trail_length": 2,
      "version": 2
    }
  }
}
```

The instance moved from `red` to `green`. The trail now has 2 entries and the version incremented.

### What Happens When You Try an Invalid Transition?

Try moving directly from `green` to `red` (skipping `yellow`):

```bash
> TRANSITION "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO red
```

```json
{
  "success": false,
  "error": "No transition defined from 'green' to 'red'"
}
```

SMQL enforces the transition graph you defined. Only the transitions you declare are allowed.

## Step 4: Complete the Cycle

Continue through the remaining states:

```bash
> TRANSITION "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO yellow
> TRANSITION "01J5X7K2P3Q4R5S6T7U8V9W0XY" TO red
```

The traffic light is back to `red` with `trail_length: 4`.

## Step 5: Query the Instance

Retrieve the current state of your traffic light:

::: code-group
```bash [REPL]
> GET TrafficLight "01J5X7K2P3Q4R5S6T7U8V9W0XY"
```

```bash [curl]
curl -X POST http://localhost:4200/execute \
  -H 'Content-Type: application/json' \
  -d '{
    "smql": "GET TrafficLight \"01J5X7K2P3Q4R5S6T7U8V9W0XY\""
  }'
```
:::

```json
{
  "success": true,
  "result": {
    "id": "01J5X7K2P3Q4R5S6T7U8V9W0XY",
    "machine": "TrafficLight",
    "state": "red",
    "data": {},
    "trail_length": 4,
    "version": 4
  }
}
```

## Step 6: View the Trail

Every transition is recorded in an immutable audit trail:

```bash
> TRAIL OF TrafficLight "01J5X7K2P3Q4R5S6T7U8V9W0XY"
```

```json
{
  "success": true,
  "result": {
    "count": 4,
    "entries": [
      { "sequence": 0, "from_state": "", "to_state": "red", "timestamp": "2026-02-16T10:00:00Z" },
      { "sequence": 1, "from_state": "red", "to_state": "green", "timestamp": "2026-02-16T10:01:00Z" },
      { "sequence": 2, "from_state": "green", "to_state": "yellow", "timestamp": "2026-02-16T10:02:00Z" },
      { "sequence": 3, "from_state": "yellow", "to_state": "red", "timestamp": "2026-02-16T10:03:00Z" }
    ]
  }
}
```

Sequence 0 is always the **spawn event** — it has an empty `from_state` because the instance didn't exist before.

## Step 7: Find All Instances

If you spawn multiple traffic lights, you can query across all of them:

```bash
> SPAWN TrafficLight {}
> SPAWN TrafficLight {}
> FIND TrafficLight WHERE STATE IS red
```

```json
{
  "success": true,
  "result": {
    "count": 3,
    "instances": [
      { "id": "01J5X7K2P3Q4R5S6T7U8V9W0XY", "state": "red" },
      { "id": "01J5Y8L3Q4R5S6T7U8V9W0XYZA", "state": "red" },
      { "id": "01J5Z9M4R5S6T7U8V9W0XYZABC", "state": "red" }
    ]
  }
}
```

## Terminal States

Let's define a second machine to see how terminal states work:

```bash
> DEFINE MACHINE OneShotTask (
    STATES { pending, done }
    INITIAL STATE pending
    TERMINAL STATES { done }
    TRANSITIONS {
      pending -> done {}
    }
  )
> SPAWN OneShotTask {}
> TRANSITION "<id>" TO done
```

Once in `done` (a terminal state), no further transitions are possible:

```bash
> TRANSITION "<id>" TO pending
```

```json
{
  "success": false,
  "error": "Instance is in terminal state 'done'"
}
```

Terminal states represent the end of a lifecycle. The instance is frozen — you can still query it and view its trail, but it can never change state again.

## What You Learned

| Concept | Summary |
|---------|---------|
| `DEFINE MACHINE` | Creates a blueprint with states, transitions, and rules |
| `SPAWN` | Creates a new instance in the initial state |
| `TRANSITION` | Moves an instance from one state to another (if allowed) |
| `GET` | Retrieves the current state and data of an instance |
| `TRAIL` | Views the full history of state changes |
| `FIND ... WHERE` | Searches for instances matching a condition |
| Terminal states | End states where no further transitions are possible |

## Next Step

Your traffic light has no data and no rules about who can change the light. In the [next tutorial](./adding-data-and-guards), you'll add typed data fields, validation constraints, and guard conditions that control when transitions are allowed.
