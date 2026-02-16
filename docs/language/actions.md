# Actions

Actions are side effects triggered after a successful transition. They execute asynchronously and do not block the transition.

## Syntax

```sql
source -> target {
  ACTION : <action_call>
}
```

## Action Types

### LOG

Write a structured log entry:

```sql
ACTION : LOG("Ticket escalated by {ACTOR}")
```

String interpolation with `{field_name}` is supported.

### NOTIFY

Send a notification to a target:

```sql
ACTION : NOTIFY(assignee, "ticket.assigned")
ACTION : NOTIFY(customer_id, "ticket.resolved")
ACTION : NOTIFY(PARENT.customer, "item.backordered")
```

The first argument is the recipient (a data field reference), the second is the notification type.

### EMIT

Publish an event to the event bus:

```sql
ACTION : EMIT("order.placed")
ACTION : EMIT("order.placed", { order: SELF })
```

Events are broadcast via the EventBus and can be received by WebSocket subscribers. The optional second argument is a data payload.

### WEBHOOK

Call an external HTTP endpoint:

```sql
ACTION : WEBHOOK("https://api.example.com/hooks", { event: "shipped" })
```

::: info
Actions are fire-and-forget. A failing action does not roll back the transition.
:::

## Multiple Actions

A transition can have multiple actions:

```sql
in_progress -> resolved {
  ACTION : NOTIFY(customer_id, "ticket.resolved")
  ACTION : EMIT("ticket.resolved", { ticket: SELF })
  ACTION : LOG("Resolved by {ACTOR}")
}
```
