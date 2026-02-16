# Why SMQL?

SMQL is the right choice when your entities have well-defined lifecycles. Here are common patterns where it excels.

## Use Cases

### Customer Support Tickets

Tickets flow through `open → triaged → in_progress → resolved → closed` with guard rules (only assignee can resolve), timeouts (auto-close after 7 days), and audit trails (who escalated and when).

### Order Processing

Orders involve parent-child composition: an `Order` contains `LineItem` children and an optional `Shipment`. Guards like `ALL(items, STATE IS confirmed)` ensure all items are ready before fulfillment.

### CI/CD Pipelines

Three-level composition: `Pipeline → Stage → Job`. A pipeline passes when all stages pass; a stage fails when any job fails. SMQL's `ALL()` and `ANY()` predicates express this naturally.

### Approval Workflows

Documents move through `draft → submitted → under_review → approved/rejected`. Guards enforce role-based rules (`ACTOR.role == "reviewer"`), and timeouts escalate stale reviews.

### Billing & Subscriptions

Subscriptions cycle through `trial → active → past_due → cancelled → expired`. Timeouts handle grace periods. Data constraints ensure payment methods are set before activation.

### IoT Device Management

Devices transition through `provisioned → online → degraded → offline → decommissioned`. Sensor data is stored as instance data, and transition trails provide a complete operational history.

## Why Not Just SQL?

You *can* model state machines in SQL. Many teams do. But you'll end up building:

1. **A transition validation layer** — checking which states can reach which
2. **A guard evaluation system** — checking preconditions before transitions
3. **An audit trail table** — recording who did what when
4. **A timer service** — handling timeouts and scheduled transitions
5. **A notification dispatch** — triggering side effects on state changes

SMQL provides all five out of the box, with a single declaration.

## When SMQL Is Not the Right Fit

- **Simple CRUD** — if your entities don't have meaningful state transitions, use a regular database
- **Continuous data** — time-series data, metrics, logs belong in specialized stores
- **Graph traversal** — complex relationship queries are better served by graph databases
- **Full-text search** — use Elasticsearch or similar for search-heavy workloads
