# smql-ast — Session Notes

## 2026-02-15 — Initial Implementation

### What was done
- All core AST types implemented across 7 source files
- 71 unit tests covering ser/de, Display, Clone, PartialEq

### File structure
- `types.rs` — TypeDefinition, Constraint, DefaultValue, DataFieldDefinition, SortClause, AggregateFunction
- `value.rs` — Value enum (runtime values), SmqlDuration
- `expression.rs` — Expression with ExpressionKind, BinaryOperator, UnaryOperator
- `machine.rs` — MachineDefinition, StateDefinition, TransitionDefinition, Action, HookDefinition, RoleDefinition
- `query.rs` — Query AST nodes (Get, Find, Aggregate, Trail, Paths, Funnel, ComparePaths)
- `command.rs` — Command AST nodes (DefineMachine, Spawn, Transition, AlterMachine), Statement
- `error.rs` — SmqlError, TransitionDeniedError, GuardFailure
- `span.rs` — Span for source location tracking

### Design decisions
- SmqlDuration uses u64 seconds internally, displays in largest-unit-first format (7d, 3d, 72h → all valid)
- Value::Money stores minor units (cents) as i64 with currency string
- Expression carries optional Span for error reporting
- TransitionSource enum supports State, Any{except}, Group variants
- All types derive Serialize/Deserialize for persistence and wire protocol
