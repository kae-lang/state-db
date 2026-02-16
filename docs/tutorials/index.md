# Tutorials

Learn SMQL step by step, from your first machine to production deployment.

## Learning Path

Each tutorial builds on the previous one. Start at the beginning and work through them in order, or jump to the topic you need.

| # | Tutorial | Level | What You'll Learn |
|---|---------|-------|-------------------|
| 1 | [Your First Machine](./your-first-machine) | Beginner | Define states, transitions, spawn instances, query state |
| 2 | [Adding Data & Guards](./adding-data-and-guards) | Beginner+ | Typed data fields, constraints, guard conditions, WITH clause |
| 3 | [Timeouts & Hooks](./timeouts-and-hooks) | Intermediate | Automatic timers, lifecycle hooks, event streaming |
| 4 | [Composition Patterns](./composition-patterns) | Intermediate+ | Parent-child machines, ALL/ANY predicates, CASCADE |
| 5 | [Queries & Analytics](./queries-and-analytics) | Intermediate+ | FIND filters, AGGREGATE, TRAIL, PATHS, FUNNEL |
| 6 | [Production Deployment](./production-deployment) | Advanced | RocksDB, Prometheus, WebSocket, SDK, ALTER MACHINE |

## Prerequisites

Before starting, build SMQL from source:

```bash
git clone <repo-url> && cd smql-engine
cargo build --release
```

The tutorials use a mix of REPL, `curl`, and SDK examples. You can follow along with whichever tool you prefer.

## Quick Reference

Throughout the tutorials, you'll see examples in multiple formats:

::: code-group
```bash [REPL]
smql repl
> SPAWN Counter {}
```

```bash [curl]
curl -X POST http://localhost:4200/execute \
  -H 'Content-Type: application/json' \
  -d '{"smql": "SPAWN Counter {}"}'
```

```rust [SDK]
let inst = client.spawn("Counter", json!({})).await?;
```
:::

Pick the one that works best for you. The REPL is the fastest way to experiment.
