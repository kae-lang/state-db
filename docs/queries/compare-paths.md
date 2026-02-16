# COMPARE PATHS

Compare transition paths segmented by a data field.

## Syntax

```sql
COMPARE PATHS MachineName SEGMENT BY field_name
```

## Example

```sql
COMPARE PATHS SupportTicket SEGMENT BY priority
```

This groups path analysis by the value of `priority`, allowing you to compare how `low`, `medium`, `high`, and `critical` tickets flow through states differently.
