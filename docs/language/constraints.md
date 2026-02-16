# Constraints

Constraints are applied to data fields after the `->` arrow. Multiple constraints are separated by commas.

```sql
DATA {
  subject : TEXT -> REQUIRED, MAX(200)
  count   : INT  -> MIN(0), DEFAULT(0)
}
```

## Available Constraints

| Constraint | Applies To | Description |
|-----------|-----------|-------------|
| `REQUIRED` | All types | Field must be set at spawn time |
| `OPTIONAL` | All types | Field may be null (default if no `REQUIRED`) |
| `DEFAULT(value)` | All types | Value used when not provided at spawn |
| `MIN(n)` | INT, FLOAT, LIST, SET | Minimum value or collection size |
| `MAX(n)` | INT, FLOAT, TEXT, LIST, SET | Maximum value or string/collection length |
| `RANGE(min, max)` | INT, FLOAT | Shorthand for MIN + MAX |
| `UNIQUE` | TEXT, UUID, INT | Value must be unique across all instances |
| `PATTERN(regex)` | TEXT | Value must match the regex pattern |

## REQUIRED vs OPTIONAL

By default, fields are optional. A `REQUIRED` field must be provided when spawning an instance.

```sql
DATA {
  email : TEXT -> REQUIRED          -- must be set at SPAWN
  phone : TEXT -> OPTIONAL          -- can be null
  notes : TEXT                      -- also optional (implicit)
}
```

## DEFAULT

`DEFAULT` provides a value when the field is not explicitly set at spawn time. Fields with defaults are implicitly optional at spawn.

```sql
priority : ENUM(low, medium, high) -> DEFAULT(medium)
tags     : SET(TEXT)               -> DEFAULT({})
count    : INT                     -> DEFAULT(0)
```

## MIN and MAX

For numeric types, these constrain the value range. For strings, `MAX` constrains length. For collections, they constrain the number of elements.

```sql
age     : INT      -> MIN(0), MAX(150)
name    : TEXT     -> MAX(100)
items   : LIST(TEXT) -> MIN(1), MAX(10)
```

## RANGE

A convenience for `MIN` + `MAX` combined:

```sql
satisfaction : INT -> RANGE(1, 5)
-- equivalent to: INT -> MIN(1), MAX(5)
```

## UNIQUE

Ensures no two instances of the same machine have the same value for this field.

```sql
email : TEXT -> REQUIRED, UNIQUE
```

## PATTERN

Validates text against a regular expression.

```sql
email : TEXT -> REQUIRED, PATTERN("^[^@]+@[^@]+$")
```

::: tip
Constraints are validated at spawn time and when fields are modified via MUTATE.
:::
