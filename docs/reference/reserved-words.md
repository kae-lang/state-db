# Reserved Words

The following identifiers are reserved by the SMQL lexer and cannot be used as machine names, state names, or field names. Keywords are case-insensitive — `DEFINE`, `define`, and `Define` are all treated as the keyword.

## Commands & Definitions

| Keyword | Usage |
|---------|-------|
| `DEFINE` | Define a machine |
| `MACHINE` | Machine declaration |
| `ALTER` | Alter a machine schema |
| `ADD` | Add state/transition/data |
| `REMOVE` | Remove state/transition/data |
| `MODIFY` | Modify existing elements |
| `BACKFILL` | Backfill data during ALTER |
| `MIGRATE` | Migrate instances |
| `SPAWN` | Create a new instance |
| `BATCH` | Batch operations |
| `TRANSITION` | Move between states |
| `TRY` | Attempt transition without error |

## Machine Structure

| Keyword | Usage |
|---------|-------|
| `DATA` | Data block |
| `STATES` | States block |
| `INITIAL` | Initial state modifier |
| `STATE` | State keyword |
| `TERMINAL` | Terminal state modifier |
| `TRANSITIONS` | Transitions block |
| `CHILDREN` | Children block |
| `PARENT` | Parent declaration |
| `HOOKS` | Hooks block |
| `ROLES` | Roles block |

## Transition Clauses

| Keyword | Usage |
|---------|-------|
| `GUARD` | Guard expression |
| `ACTION` | Side effect |
| `TIMEOUT` | Automatic transition timer |
| `MUTATE` | Data modification |
| `TO` | Target state |
| `FROM` | Source reference |
| `AS` | Actor identity |
| `WITH` | Additional data |
| `MEMO` | Transition note |
| `THROUGH` | Multi-hop transition |
| `THEN` | Chained operation |
| `OR_STAY` | Stay on failure |
| `CASCADE` | Propagate to children |

## Queries

| Keyword | Usage |
|---------|-------|
| `GET` | Get single instance |
| `FIND` | Search instances |
| `AGGREGATE` | Aggregate query |
| `TRAIL` | Transition history |
| `PATHS` | Path analysis |
| `FUNNEL` | Funnel analysis |
| `COMPARE` | Compare paths |
| `COUNT` | Count aggregate |
| `SUM` | Sum aggregate |
| `AVG` | Average aggregate |
| `PERCENTILE` | Percentile aggregate |
| `WHERE` | Filter clause |
| `SORT` | Sort clause |
| `BY` | Group/sort modifier |
| `LIMIT` | Result limit |
| `OFFSET` | Result offset |
| `GROUP` | Group by |
| `MEASURE` | Aggregate measure |
| `OF` | Trail of |
| `SEGMENT` | Path segmentation |
| `ASC` | Ascending sort |
| `DESC` | Descending sort |
| `SUBSCRIBE` | WebSocket subscribe |
| `DELIVER` | Deliver keyword |

## Logical & Comparison

| Keyword | Usage |
|---------|-------|
| `AND` | Logical AND |
| `OR` | Logical OR |
| `NOT` | Logical NOT |
| `IN` | Membership test |
| `IS` | State/null check |
| `SET` | IS SET / IS NOT SET |
| `NULL` | Null literal |
| `TRUE` | Boolean true |
| `FALSE` | Boolean false |

## Data Types

| Keyword | Usage |
|---------|-------|
| `TEXT` | String type |
| `INT` | Integer type |
| `FLOAT` | Float type |
| `BOOL` | Boolean type |
| `UUID` | UUID type |
| `DATE` | Date type |
| `DATETIME` | DateTime type |
| `DURATION` | Duration type |
| `MONEY` | Money type |
| `BLOB` | Binary data type |
| `JSON` | JSON type |
| `ENUM` | Enum type |
| `REF` | Reference type |
| `LIST` | List type |
| `MAP` | Map type |

## Constraints

| Keyword | Usage |
|---------|-------|
| `REQUIRED` | Field is required |
| `OPTIONAL` | Field is optional |
| `DEFAULT` | Default value |
| `MAX` | Maximum constraint |
| `MIN` | Minimum constraint |
| `RANGE` | Range constraint |
| `UNIQUE` | Uniqueness constraint |
| `PATTERN` | Regex pattern constraint |

## Wildcards & Predicates

| Keyword | Usage |
|---------|-------|
| `ANY` | Wildcard source / any predicate |
| `ALL` | All children predicate |
| `EXCEPT` | Exclude from wildcard |

## Hooks & Events

| Keyword | Usage |
|---------|-------|
| `ON` | Hook trigger |
| `BEFORE` | Before hook |
| `AFTER` | After hook |
| `EACH` | Each transition modifier |
| `ENTER` | State enter hook |
| `EXIT` | State exit hook |
| `DWELL` | Dwell timer (reserved) |
| `SIGNAL` | Signal parent |
| `EMIT` | Emit event |
| `NOTIFY` | Send notification |
| `LOG` | Log action |
| `WEBHOOK` | HTTP callback |

## Built-in Functions & References

| Keyword | Usage |
|---------|-------|
| `SELF` | Current instance reference |
| `ACTOR` | Current actor reference |
| `NOW` | Current timestamp |
| `TODAY` | Current date |
| `CONTAINS` | Collection membership |

## Filter Predicates

| Keyword | Usage |
|---------|-------|
| `STUCK_IN` | Stuck in state predicate |
| `TIMEOUT_REMAINING` | Remaining timeout |
| `HAS_VISITED` | Has visited state |
| `NEVER_VISITED` | Never visited state |
| `ALIVE` | Non-terminal predicate |
| `TERMINATED` | Terminal predicate |
| `DELETED` | Deleted predicate |
