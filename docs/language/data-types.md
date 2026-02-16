# Data Types

SMQL supports 16 data types for instance fields.

## Scalar Types

| Type | Description | JSON Representation | Example |
|------|-------------|-------------------|---------|
| `TEXT` | Unicode string | `"hello"` | `name : TEXT` |
| `INT` | 64-bit signed integer | `42` | `count : INT` |
| `FLOAT` | 64-bit floating point | `3.14` | `rating : FLOAT` |
| `BOOL` | Boolean | `true` / `false` | `active : BOOL` |
| `UUID` | UUID v4/v7 | `"550e8400-..."` | `user_id : UUID` |
| `DATE` | Calendar date | `"2026-02-16"` | `due_date : DATE` |
| `DATETIME` | Date with time (UTC) | `"2026-02-16T10:00:00Z"` | `created : DATETIME` |
| `DURATION` | Time duration | `"7d"` / `"24h"` | `sla : DURATION` |
| `JSON` | Arbitrary JSON | `{...}` | `metadata : JSON` |
| `BLOB` | Binary data | `{"blob_size": N}` | `attachment : BLOB` |

## Enum Type

A fixed set of allowed string values.

```sql
priority : ENUM(low, medium, high, critical)
```

The value must be one of the listed variants. Enums are stored as text internally.

## Reference Type

A reference to an instance of another machine.

```sql
assignee : REF(Agent)
```

In JSON, refs are serialized as `{"ref": "Agent#01J5..."}`.

## Money Type

A monetary amount with currency code.

```sql
total : MONEY(USD)
price : MONEY(EUR)
```

In JSON, money values are `{"amount": 9999, "currency": "USD"}`. The amount is stored as an integer (cents).

::: warning
Money values cannot be compared directly with INT. A guard like `total > 0` will fail if `total` is `MONEY(USD)`. Compare money amounts explicitly.
:::

## Collection Types

### LIST

An ordered collection of values.

```sql
tags : LIST(TEXT)
```

In JSON: `["tag1", "tag2"]`.

### SET

An unordered collection of unique values.

```sql
categories : SET(TEXT)
```

In JSON: `["a", "b"]` (deduplicated).

### MAP

A key-value mapping.

```sql
metadata : MAP(TEXT, TEXT)
```

In JSON: `{"key": "value"}`.

## Type Constraints

All types support [constraints](./constraints) like `REQUIRED`, `OPTIONAL`, `DEFAULT`, `MIN`, `MAX`, `RANGE`, `UNIQUE`, and `PATTERN`.
