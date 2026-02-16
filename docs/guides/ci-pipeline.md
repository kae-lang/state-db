# Three-Level CI/CD Pipeline

This guide walks through a CI/CD pipeline modeled as a three-level machine hierarchy: Pipeline, Stage, and Job. It demonstrates SMQL's composition features at depth, where `ALL()` and `ANY()` predicates cascade upward through multiple layers of parent-child relationships.

## Prerequisites

Start the SMQL server:

```bash
smql-server --port 8080
```

---

## Architecture Overview

```
Pipeline
  |
  +-- stages: LIST(Stage) -> MIN(1)
        |
        +-- jobs: LIST(Job) -> MIN(1)
```

The pass/fail logic flows upward:
- A **Job** passes or fails on its own (no children).
- A **Stage** passes when `ALL(jobs, STATE IS passed)` and fails when `ANY(jobs, STATE IS failed)`.
- A **Pipeline** passes when `ALL(stages, STATE IS passed)` and fails when `ANY(stages, STATE IS failed)`.

This mirrors how real CI systems like GitHub Actions or GitLab CI work.

---

## Step 1: Define the Machines

### Pipeline

```sql
DEFINE MACHINE Pipeline (
  DATA {
    repo     : TEXT -> REQUIRED
    branch   : TEXT -> REQUIRED
    commit   : TEXT -> REQUIRED
    trigger  : ENUM(push, pr, manual, schedule) -> DEFAULT(push)
  }

  STATES { queued, running, passed, failed, cancelled }
  INITIAL STATE queued
  TERMINAL STATES { passed, failed, cancelled }

  CHILDREN {
    stages : LIST(Stage) -> MIN(1)
  }

  TRANSITIONS {
    queued -> running {}

    running -> passed {
      GUARD : ALL(stages, STATE IS passed)
    }

    running -> failed {
      GUARD : ANY(stages, STATE IS failed)
    }

    ANY -> cancelled {
      EXCEPT FROM { passed, failed }
    }
  }
)
```

### Stage

```sql
DEFINE MACHINE Stage (
  PARENT : Pipeline

  DATA {
    name  : TEXT -> REQUIRED
    order : INT  -> REQUIRED
  }

  STATES { pending, running, passed, failed, skipped }
  INITIAL STATE pending
  TERMINAL STATES { passed, failed, skipped }

  CHILDREN {
    jobs : LIST(Job) -> MIN(1)
  }

  TRANSITIONS {
    pending -> running {}
    running -> passed {
      GUARD : ALL(jobs, STATE IS passed)
    }
    running -> failed {
      GUARD : ANY(jobs, STATE IS failed)
    }
    pending -> skipped {}
  }
)
```

### Job

```sql
DEFINE MACHINE Job (
  PARENT : Stage

  DATA {
    name    : TEXT -> REQUIRED
    image   : TEXT -> OPTIONAL
    command : TEXT -> REQUIRED
  }

  STATES { pending, running, passed, failed }
  INITIAL STATE pending
  TERMINAL STATES { passed, failed }

  TRANSITIONS {
    pending -> running {}
    running -> passed {}
    running -> failed {}
  }
)
```

Jobs are leaf nodes with no children and no guards. They represent the actual work units (compile, test, lint, deploy).

### Register All Machines

```bash
# Register Pipeline
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "DEFINE MACHINE Pipeline ( DATA { repo: TEXT -> REQUIRED, branch: TEXT -> REQUIRED, commit: TEXT -> REQUIRED, trigger: ENUM(push, pr, manual, schedule) -> DEFAULT(push) } STATES { queued, running, passed, failed, cancelled } INITIAL STATE queued TERMINAL STATES { passed, failed, cancelled } CHILDREN { stages: LIST(Stage) -> MIN(1) } TRANSITIONS { queued -> running {} running -> passed { GUARD: ALL(stages, STATE IS passed) } running -> failed { GUARD: ANY(stages, STATE IS failed) } ANY -> cancelled { EXCEPT FROM { passed, failed } } } )"}'

# Register Stage
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "DEFINE MACHINE Stage ( PARENT: Pipeline DATA { name: TEXT -> REQUIRED, order: INT -> REQUIRED } STATES { pending, running, passed, failed, skipped } INITIAL STATE pending TERMINAL STATES { passed, failed, skipped } CHILDREN { jobs: LIST(Job) -> MIN(1) } TRANSITIONS { pending -> running {} running -> passed { GUARD: ALL(jobs, STATE IS passed) } running -> failed { GUARD: ANY(jobs, STATE IS failed) } pending -> skipped {} } )"}'

# Register Job
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "DEFINE MACHINE Job ( PARENT: Stage DATA { name: TEXT -> REQUIRED, image: TEXT -> OPTIONAL, command: TEXT -> REQUIRED } STATES { pending, running, passed, failed } INITIAL STATE pending TERMINAL STATES { passed, failed } TRANSITIONS { pending -> running {} running -> passed {} running -> failed {} } )"}'
```

---

## Step 2: Create the Pipeline Hierarchy

### Spawn the Pipeline

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "SPAWN Pipeline { repo: \"acme/backend\", branch: \"main\", commit: \"abc123def\" }"
  }'
```

```json
{
  "success": true,
  "result": {
    "id": "01JMPIPE00000000000000001",
    "machine": "Pipeline",
    "state": "queued",
    "data": {
      "repo": "acme/backend",
      "branch": "main",
      "commit": "abc123def",
      "trigger": "push"
    },
    "trail_length": 1,
    "version": 1
  }
}
```

The `trigger` field defaults to `"push"` since we did not provide it.

### Add Stages

```bash
# Stage 1: Build
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "SPAWN Stage { name: \"build\", order: 1 } PARENT Pipeline \"01JMPIPE00000000000000001\""
  }'
# Returns id: "01JMSTG000000000000000001"

# Stage 2: Test
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "SPAWN Stage { name: \"test\", order: 2 } PARENT Pipeline \"01JMPIPE00000000000000001\""
  }'
# Returns id: "01JMSTG000000000000000002"

# Stage 3: Deploy
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "SPAWN Stage { name: \"deploy\", order: 3 } PARENT Pipeline \"01JMPIPE00000000000000001\""
  }'
# Returns id: "01JMSTG000000000000000003"
```

### Add Jobs to Stages

```bash
# Build stage: compile job
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "SPAWN Job { name: \"compile\", command: \"cargo build --release\" } PARENT Stage \"01JMSTG000000000000000001\""
  }'
# Returns id: "01JMJOB000000000000000001"

# Test stage: unit tests
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "SPAWN Job { name: \"unit-tests\", command: \"cargo test\" } PARENT Stage \"01JMSTG000000000000000002\""
  }'
# Returns id: "01JMJOB000000000000000002"

# Test stage: lint
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "SPAWN Job { name: \"lint\", command: \"cargo clippy\" } PARENT Stage \"01JMSTG000000000000000002\""
  }'
# Returns id: "01JMJOB000000000000000003"

# Deploy stage: deploy job
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "SPAWN Job { name: \"deploy-prod\", command: \"kubectl apply -f deploy.yaml\" } PARENT Stage \"01JMSTG000000000000000003\""
  }'
# Returns id: "01JMJOB000000000000000004"
```

---

## Step 3: Run the Pipeline

### Start the Pipeline

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "smql": "TRANSITION \"01JMPIPE00000000000000001\" TO running"
  }'
```

### Run and Pass the Build Stage

```bash
# Start build stage
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMSTG000000000000000001\" TO running"}'

# Start compile job
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMJOB000000000000000001\" TO running"}'

# Compile job passes
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMJOB000000000000000001\" TO passed"}'

# Build stage passes (ALL jobs passed)
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMSTG000000000000000001\" TO passed"}'
```

The stage `running -> passed` guard checks `ALL(jobs, STATE IS passed)`. Since the compile job is the only job in the build stage and it passed, the guard succeeds.

### Run the Test Stage -- With a Failure

```bash
# Start test stage
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMSTG000000000000000002\" TO running"}'

# Start both test jobs
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMJOB000000000000000002\" TO running"}'

curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMJOB000000000000000003\" TO running"}'

# Unit tests pass
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMJOB000000000000000002\" TO passed"}'

# Lint FAILS
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMJOB000000000000000003\" TO failed"}'
```

### Stage Fails Due to ANY()

Now try to pass the test stage:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMSTG000000000000000002\" TO passed"}'
```

```json
{
  "success": false,
  "error": "Transition denied: guard failed: ALL(jobs, STATE IS passed)"
}
```

The `ALL(jobs, STATE IS passed)` guard fails because the lint job is in `failed`, not `passed`. Instead, transition the stage to `failed`:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMSTG000000000000000002\" TO failed"}'
```

```json
{
  "success": true,
  "result": {
    "from_state": "running",
    "to_state": "failed"
  }
}
```

The `running -> failed` guard is `ANY(jobs, STATE IS failed)`. Since the lint job is failed, this guard passes.

### Pipeline Fails

With one stage failed, the pipeline cannot pass:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMPIPE00000000000000001\" TO passed"}'
```

```json
{
  "success": false,
  "error": "Transition denied: guard failed: ALL(stages, STATE IS passed)"
}
```

Instead, fail the pipeline:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMPIPE00000000000000001\" TO failed"}'
```

```json
{
  "success": true,
  "result": {
    "from_state": "running",
    "to_state": "failed"
  }
}
```

---

## Alternate Path: All Green

If all jobs pass, the pipeline succeeds. Here is the complete sequence for a passing pipeline:

```bash
# 1. Start pipeline
TRANSITION "<pipeline_id>" TO running

# 2. Build stage
TRANSITION "<build_stage_id>" TO running
TRANSITION "<compile_job_id>" TO running
TRANSITION "<compile_job_id>" TO passed
TRANSITION "<build_stage_id>" TO passed     # ALL(jobs) passed

# 3. Test stage
TRANSITION "<test_stage_id>" TO running
TRANSITION "<unit_test_job_id>" TO running
TRANSITION "<lint_job_id>" TO running
TRANSITION "<unit_test_job_id>" TO passed
TRANSITION "<lint_job_id>" TO passed
TRANSITION "<test_stage_id>" TO passed      # ALL(jobs) passed

# 4. Deploy stage
TRANSITION "<deploy_stage_id>" TO running
TRANSITION "<deploy_job_id>" TO running
TRANSITION "<deploy_job_id>" TO passed
TRANSITION "<deploy_stage_id>" TO passed    # ALL(jobs) passed

# 5. Pipeline passes
TRANSITION "<pipeline_id>" TO passed        # ALL(stages) passed
```

---

## Skipping a Stage

Stages can be skipped directly from `pending`:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMSTG000000000000000003\" TO skipped"}'
```

```json
{
  "success": true,
  "result": {
    "from_state": "pending",
    "to_state": "skipped"
  }
}
```

The `skipped` state is terminal. This is useful for conditional stages (e.g., deploy only on the main branch).

---

## CASCADE Cancellation

Cancel the entire pipeline and all its children:

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRANSITION \"01JMPIPE00000000000000001\" TO cancelled CASCADE"}'
```

CASCADE propagates through all three levels:
1. Pipeline transitions to `cancelled`.
2. For each Stage child, SMQL tries to transition it to its first terminal state (`passed`). If the guard fails, the stage may remain in its current state.
3. For each Job child of each stage, the same process applies.

Because CASCADE uses `try_transition` and tries only the first terminal state, it is best-effort. In a real pipeline system, you would typically cancel jobs individually before cancelling the stage and pipeline. Alternatively, design your machines with `cancelled` as the first terminal state so CASCADE picks it up first.

---

## Querying the Pipeline

### Find All Failed Jobs

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "FIND Job WHERE STATE IS failed"}'
```

```json
{
  "success": true,
  "result": {
    "count": 1,
    "instances": [
      {
        "id": "01JMJOB000000000000000003",
        "state": "failed",
        "data": { "name": "lint", "command": "cargo clippy" }
      }
    ]
  }
}
```

### Aggregate Jobs by State

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "AGGREGATE Job MEASURE COUNT() GROUP BY state"}'
```

```json
{
  "success": true,
  "result": {
    "rows": [
      { "group": { "state": "pending" }, "measures": { "count": 1 } },
      { "group": { "state": "passed" }, "measures": { "count": 2 } },
      { "group": { "state": "failed" }, "measures": { "count": 1 } }
    ]
  }
}
```

### Pipeline Trail

```bash
curl -s -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{"smql": "TRAIL OF Pipeline \"01JMPIPE00000000000000001\""}'
```

```json
{
  "success": true,
  "result": {
    "count": 3,
    "entries": [
      { "sequence": 0, "from_state": "", "to_state": "queued", "timestamp": "2026-02-16T10:00:00Z" },
      { "sequence": 1, "from_state": "queued", "to_state": "running", "timestamp": "2026-02-16T10:00:05Z" },
      { "sequence": 2, "from_state": "running", "to_state": "failed", "timestamp": "2026-02-16T10:05:30Z" }
    ]
  }
}
```

---

## How ALL() and ANY() Work at Each Level

The predicate evaluation flows upward through the hierarchy:

```
                        Pipeline
                           |
         ALL(stages, STATE IS passed) -- pipeline passes
         ANY(stages, STATE IS failed) -- pipeline fails
                           |
              +------------+------------+
              |                         |
          Stage: build              Stage: test
              |                         |
    ALL(jobs, passed)          ALL(jobs, passed) -- stage passes
    ANY(jobs, failed)          ANY(jobs, failed) -- stage fails
              |                         |
         Job: compile          Job: unit-tests   Job: lint
         (passed)              (passed)           (failed)
```

Key rules:
- `ALL()` over an empty set returns `true` (vacuous truth). A stage with zero jobs would always pass its `ALL(jobs, ...)` guard.
- `ANY()` over an empty set returns `false`. A stage with zero jobs would never trigger its `ANY(jobs, ...)` failure guard.
- Predicates are evaluated live at transition time. There is no caching -- each transition re-queries the current state of all children.
