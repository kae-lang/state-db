# smql run

Execute SMQL statements from a `.smql` file. Supports instance ID references so that statements later in the file can refer to instances spawned earlier.

## Usage

```bash
smql run [OPTIONS] <FILE>
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<FILE>` | Path to the `.smql` file to execute (positional, required) |

## Options

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--storage` | `-s` | `memory` | Storage backend -- `"memory"` or a filesystem path for RocksDB |

## ID References

Files executed with `smql run` support `$1`, `$2`, etc. as placeholders for instance IDs. These references resolve to the IDs of previously spawned instances within the same file, using 1-based indexing:

- `$1` resolves to the ID returned by the first `SPAWN` statement
- `$2` resolves to the ID returned by the second `SPAWN` statement
- And so on

This lets you define a machine, spawn instances, and transition them all in a single file without manually copying IDs.

## Examples

Given a file `test.smql`:

```sql
DEFINE MACHINE Task (
  DATA { title: TEXT -> REQUIRED }
  STATES { todo, doing, done }
  INITIAL STATE todo
  TERMINAL STATES { done }
  TRANSITIONS {
    todo -> doing {}
    doing -> done {}
  }
)

SPAWN Task { title: "First task" }
TRANSITION $1 TO doing
TRANSITION $1 TO done
```

Run it:

```bash
smql run test.smql
```

The `$1` in the `TRANSITION` statements resolves to the ID returned by the `SPAWN Task` statement.

### Multiple Spawns

```sql
DEFINE MACHINE Task (
  DATA { title: TEXT -> REQUIRED }
  STATES { todo, done }
  INITIAL STATE todo
  TERMINAL STATES { done }
  TRANSITIONS { todo -> done {} }
)

SPAWN Task { title: "First" }
SPAWN Task { title: "Second" }
TRANSITION $1 TO done
TRANSITION $2 TO done
```

Here `$1` refers to the "First" instance and `$2` refers to the "Second" instance.

### With Persistent Storage

```bash
smql run test.smql --storage ./data
```
