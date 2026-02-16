# smql repl

Open the interactive SMQL REPL (Read-Eval-Print Loop) powered by rustyline.

Running `smql` with no subcommand also opens the REPL.

## Usage

```bash
smql repl [OPTIONS]
smql  # equivalent — opens the REPL
```

## Options

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--storage` | `-s` | `memory` | Storage backend -- `"memory"` or a filesystem path for RocksDB |

## Multiline Input

The REPL supports multiline input. Statements are executed when the input forms a complete SMQL statement.

## Dot Commands

The REPL provides built-in dot commands for convenience:

| Command | Description |
|---------|-------------|
| `.help` | Show available dot commands |
| `.machines` | List all defined machines |
| `.quit` | Exit the REPL |

## Examples

Start the REPL with in-memory storage:

```bash
smql repl
```

Start the REPL with RocksDB-backed storage:

```bash
smql repl --storage ./data
```

### Example Session

```
$ smql
smql> DEFINE MACHINE Task (
   ...>   DATA { title: TEXT -> REQUIRED }
   ...>   STATES { todo, doing, done }
   ...>   INITIAL STATE todo
   ...>   TERMINAL STATES { done }
   ...>   TRANSITIONS {
   ...>     todo -> doing {}
   ...>     doing -> done {}
   ...>   }
   ...> )
Machine "Task" defined.

smql> SPAWN Task { title: "Write docs" }
Spawned instance 01J5A...

smql> .machines
- Task

smql> .quit
```
