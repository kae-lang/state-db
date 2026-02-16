# CLI Overview

The `smql` command-line interface provides tools for defining, running, and managing SMQL state machines. It includes an HTTP server, an interactive REPL, file execution, and code generation.

## Installation

After building the workspace, the `smql` binary is available in the target directory.

## Subcommands

| Command | Description |
|---------|-------------|
| [`serve`](./serve) | Start the HTTP/JSON API server |
| [`repl`](./repl) | Open the interactive REPL |
| [`exec`](./exec) | Execute a single SMQL statement from a string |
| [`run`](./run) | Execute SMQL statements from a `.smql` file |
| [`codegen`](./codegen) | Generate typed Rust code from machine definitions |

Running `smql` with no subcommand opens the REPL.

## Global Options

All subcommands that operate on a local engine accept the `--storage` / `-s` flag to choose between an in-memory backend and a RocksDB-backed backend.

```bash
# In-memory (default)
smql repl --storage memory

# RocksDB at a filesystem path
smql repl --storage ./data
```

## Quick Start

Define a machine, spawn an instance, and transition it -- all from the command line:

```bash
# Define a machine inline
smql exec 'DEFINE MACHINE Task ( STATES { todo, done } INITIAL STATE todo TERMINAL STATES { done } TRANSITIONS { todo -> done {} } )'
```

Or write statements to a file and run them together:

```bash
# run.smql
smql run workflow.smql
```

Start the HTTP server for remote access:

```bash
smql serve --bind 0.0.0.0:8080 --storage ./data
```

Generate typed Rust client code from your machine definitions:

```bash
smql codegen --input machines/ --output src/generated
```
