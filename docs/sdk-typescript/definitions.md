# Definition Builders

The TypeScript SDK provides builder classes for all SMQL `DEFINE` statements. Each generates the correct SMQL syntax and sends it to the server via `.execute()`.

## DefineMachineBuilder

Build a complete `DEFINE MACHINE` statement with data fields, states, transitions, hooks, and roles.

```typescript
await client.defineMachine("SupportTicket")
  // Data fields
  .data("subject", "TEXT", "REQUIRED")
  .data("priority", "INT", { type: "RANGE", lo: 1, hi: 5 }, { type: "DEFAULT", value: 3 })
  .data("assignee", "TEXT", "OPTIONAL")
  // States
  .states("open", "in_progress", "resolved", "closed")
  .initialState("open")
  .terminalStates("closed")
  // Transitions with guards, actions, mutations
  .transition("open", "in_progress")
    .guard("assignee IS SET")
    .mutate("started_at", "NOW()")
    .end()
  .transition("in_progress", "resolved")
    .action('LOG("ticket resolved")')
    .end()
  .transition("resolved", "closed")
    .timeout("7d", "closed")
    .end()
  .transition("resolved", "open").end()
  // Roles
  .role("agent")
    .canSpawn()
    .canTransition("in_progress", "resolved")
    .canWrite("assignee", "priority")
    .end()
  .role("admin")
    .canAll()
    .end()
  .execute();
```

### Data Fields

```typescript
.data(name: string, type: DataType, ...constraints: Constraint[])
```

**Data types:** `"TEXT"`, `"INT"`, `"FLOAT"`, `"BOOL"`, `"UUID"`, `"DATE"`, `"DATETIME"`, `"DURATION"`, `"BLOB"`, `"JSON"`, or compound types:

```typescript
.data("tags", { type: "LIST", inner: "TEXT" })
.data("category", { type: "ENUM", variants: ["bug", "feature", "docs"] })
.data("order_ref", { type: "REF", target: "Order" })
.data("price", { type: "MONEY", currency: "USD" })
.data("metadata", { type: "MAP", key: "TEXT", value: "TEXT" })
```

**Constraints:** `"REQUIRED"`, `"OPTIONAL"`, `"UNIQUE"`, or:

```typescript
{ type: "MIN", value: 0 }
{ type: "MAX", value: 100 }
{ type: "RANGE", lo: 1, hi: 10 }
{ type: "DEFAULT", value: "pending" }  // string, number, boolean, or null
{ type: "PATTERN", regex: "^[A-Z]{3}$" }
{ type: "COMPUTED", expr: "quantity * price" }
```

### TransitionDefBuilder

Returned by `.transition(from, to)`. Chain clauses then call `.end()` to return to the parent.

| Method | Description |
|--------|-------------|
| `.guard(expr)` | Add a guard expression |
| `.action(actionStr)` | Add an action (e.g., `'LOG("msg")'`, `'EMIT("event")'`) |
| `.mutate(field, expr)` | Add a mutation (e.g., `.mutate("count", "count + 1")`) |
| `.timeout(duration, state)` | Auto-transition after duration (e.g., `"30m"`, `"24h"`) |
| `.applyPolicy(name)` | Apply a named policy |
| `.reactive(condition)` | Make transition reactive to a condition |
| `.end()` | Finalize and return to `DefineMachineBuilder` |

### HookDefBuilder

Returned by `.hook(trigger)`. Trigger strings: `"ON SPAWN"`, `"ON ENTER state"`, `"ON EXIT state"`, `"BEFORE EACH TRANSITION"`, `"AFTER EACH TRANSITION"`.

```typescript
.hook("ON ENTER resolved")
  .action('NOTIFY(self.assignee, "ticket resolved")')
  .end()
```

### RoleDefBuilder

Returned by `.role(name)`.

| Method | Description |
|--------|-------------|
| `.canSpawn()` | Allow spawning instances |
| `.canTransition(...states)` | Allow transitioning to specific states |
| `.canQuery()` | Allow query access |
| `.canAlter()` | Allow schema changes |
| `.canAll()` | Full permissions |
| `.canRead(...fields)` | Allow reading specific fields |
| `.canWrite(...fields)` | Allow writing specific fields |
| `.cannotRead(...fields)` | Deny reading specific fields |
| `.cannotWrite(...fields)` | Deny writing specific fields |
| `.end()` | Return to `DefineMachineBuilder` |

## DefinePolicyBuilder

Define reusable guard policies.

```typescript
await client.definePolicy("business_hours")
  .guard("NOW().hour >= 9 AND NOW().hour < 17")
  .execute();
```

Multiple guards are AND-combined:

```typescript
await client.definePolicy("rate_limit")
  .guard("request_count < 100")
  .guard("last_request_at < NOW() - 1s")
  .execute();
```

## DefineViewBuilder

Define a saved FIND query.

```typescript
await client.defineView("open_tickets")
  .find("SupportTicket")
  .where("STATE IS open")
  .sortBy("created_at", "DESC")
  .limit(50)
  .execute();

// Later, retrieve the view
const results = await client.getView("open_tickets");
```

## DefineProjectionBuilder

Define a materialized aggregation.

```typescript
await client.defineProjection("ticket_stats")
  .aggregate("SupportTicket")
  .count("total")
  .avg("resolution_time", "avg_resolution")
  .groupByState()
  .refreshOnTransition()
  .execute();

// Later, retrieve the projection
const stats = await client.getProjection("ticket_stats");
```

### Refresh Policies

| Method | Generated SMQL |
|--------|---------------|
| `.refreshOnTransition()` | `REFRESH ON TRANSITION` |
| `.refreshOnInterval(seconds)` | `REFRESH ON INTERVAL 60` |
| `.refreshManual()` | `REFRESH MANUAL` |

## DefineRuleBuilder

Define invariant rules that guard transitions or spawns.

```typescript
await client.defineRule("positive_amount")
  .beforeTransition("Order")
  .invariant("amount > 0", "Amount must be positive")
  .execute();
```

### Triggers

| Method | Generated SMQL |
|--------|---------------|
| `.beforeTransition(machine)` | `BEFORE TRANSITION ON Machine` |
| `.beforeSpawn(machine)` | `BEFORE SPAWN ON Machine` |
| `.beforeAnyTransition()` | `BEFORE ANY TRANSITION` |
| `.afterTransition(machine)` | `AFTER TRANSITION ON Machine` |

## DefineSubscriptionBuilder

Define server-side event subscriptions with actions.

```typescript
await client.defineSubscription("notify_shipped")
  .onTransition("Order", "confirmed", "shipped")
  .action('NOTIFY(self.customer_email, "Your order shipped!")')
  .execute();
```

### Event Types

| Method | Generated SMQL |
|--------|---------------|
| `.onEnter(state, machine)` | `ON ENTER state ON Machine` |
| `.onExit(state, machine)` | `ON EXIT state ON Machine` |
| `.onSpawn(machine)` | `ON SPAWN Machine` |
| `.onTransition(machine, from?, to?)` | `ON TRANSITION Machine [FROM x] [TO y]` |

### Conditional Actions

```typescript
.actionWhen("amount > 1000", 'LOG("high-value order shipped")')
```

## DefineSagaBuilder

Define multi-step distributed transactions with compensation.

```typescript
await client.defineSaga("checkout")
  .triggerOnEnter("submitted", "Order")
  .step("reserve_inventory")
    .transition("Inventory", "self.inventory_id", "reserved")
    .compensate("Inventory", "self.inventory_id", "available")
    .end()
  .step("charge_payment")
    .transition("Payment", "self.payment_id", "charged")
    .when("self.total > 0")
    .compensate("Payment", "self.payment_id", "refunded")
    .end()
  .onComplete('EMIT("checkout_complete")')
  .onFailure('LOG("checkout failed")')
  .execute();
```

### Saga Triggers

| Method | Generated SMQL |
|--------|---------------|
| `.triggerOnEnter(state, machine)` | `TRIGGER ON ENTER state ON Machine` |
| `.triggerOnSpawn(machine)` | `TRIGGER ON SPAWN Machine` |
| `.triggerManual()` | `TRIGGER MANUAL` |

## AlterMachineBuilder

Modify an existing machine schema with migrations.

```typescript
const result = await client.alterMachine("SupportTicket")
  .addState("archived")
  .addTransition("closed", "archived")
  .addData("archived_by", "TEXT", ["OPTIONAL"])
  .removeData("legacy_field")
  .backfill("priority", "3")
  .execute();

console.log(`New version: ${result.new_version}`);
console.log(`Instances migrated: ${result.instances_migrated}`);
```

### Operations

| Method | Generated SMQL |
|--------|---------------|
| `.addState(name)` | `ADD STATE name` |
| `.removeState(state, migrateTo)` | `REMOVE STATE state MIGRATE TO migrateTo` |
| `.addTransition(from, to)` | `ADD TRANSITION from -> to` |
| `.removeTransition(from, to)` | `REMOVE TRANSITION from -> to` |
| `.addData(field, type, constraints?, backfill?)` | `ADD DATA field : TYPE [-> constraints] [BACKFILL value]` |
| `.removeData(field)` | `REMOVE DATA field` |
| `.backfill(field, expr)` | `BACKFILL field = expr` |
