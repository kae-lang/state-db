# smql exec

Execute a single SMQL statement from a string argument.

## Usage

```bash
smql exec [OPTIONS] <STATEMENT>
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<STATEMENT>` | The SMQL statement to execute (positional, required) |

## Options

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--storage` | `-s` | `memory` | Storage backend -- `"memory"` or a filesystem path for RocksDB |

## Examples

Define a machine:

```bash
smql exec 'DEFINE MACHINE Task ( STATES { todo, done } INITIAL STATE todo TERMINAL STATES { done } TRANSITIONS { todo -> done {} } )'
```

With persistent storage so the definition is retained across invocations:

```bash
smql exec --storage ./data 'DEFINE MACHINE Task ( STATES { todo, done } INITIAL STATE todo TERMINAL STATES { done } TRANSITIONS { todo -> done {} } )'
```

Spawn an instance (requires the machine to already be defined in storage):

```bash
smql exec --storage ./data 'SPAWN Task { title: "Ship it" }'
```

## Notes

Each invocation of `smql exec` creates a fresh engine. When using the default in-memory storage, nothing persists between calls. Use `--storage` with a filesystem path to retain state across invocations.

For executing multiple statements together, see [`smql run`](./run).
