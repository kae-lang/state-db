# smql codegen

Generate typed Rust code from `.smql` machine definitions. Produces one `.rs` file per machine plus a `mod.rs` with `pub mod` declarations.

## Usage

```bash
smql codegen [OPTIONS]
```

## Options

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--input` | `-i` | *(required)* | Input `.smql` files or directories (can be specified multiple times) |
| `--output` | `-o` | `src/generated` | Output directory for generated Rust files |
| `--lang` | `-l` | `rust` | Target language (only `"rust"` is currently supported) |

## Examples

Generate code from all `.smql` files in a directory:

```bash
smql codegen --input examples/ --output src/generated
```

Specify multiple input files:

```bash
smql codegen --input machines/order.smql --input machines/ticket.smql --output src/gen
```

Use the default output directory:

```bash
smql codegen --input machines/
```

## Generated Output

For each machine definition found in the input files, `smql codegen` produces a Rust module containing typed representations of the machine's states, data fields, and transitions.

Given a machine named `SupportTicket`, the codegen produces:

```
src/generated/
  mod.rs              # pub mod declarations
  support_ticket.rs   # typed code for SupportTicket
```

### Type Mapping

SMQL data types map to Rust types as follows:

| SMQL Type | Rust Type |
|-----------|-----------|
| `TEXT` | `String` |
| `INT` | `i64` |
| `FLOAT` | `f64` |
| `BOOL` | `bool` |
| `MONEY` | `(i64, String)` |
| `REF` | `String` |
| `LIST` | `Vec<serde_json::Value>` |
| `MAP` | `std::collections::BTreeMap<String, serde_json::Value>` |

## Notes

- The `--input` flag can point to individual `.smql` files or directories. When given a directory, all `.smql` files in that directory are processed.
- Machine names are converted to `snake_case` for file names and `PascalCase` for Rust type names.
- Only the `"rust"` language target is supported at this time.
