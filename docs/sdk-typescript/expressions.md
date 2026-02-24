# Expression Builder

All SDK builders that accept filter expressions (`.where()`, `.guard()`, etc.) accept raw SMQL expression strings. For programmatic construction, the SDK also provides an `Expr` helper class.

## Raw Strings

You can always pass expression strings directly:

```typescript
client.find("Order").where("priority == 1 AND STATE IS pending")
client.find("Order").where("amount > 1000 OR region == \"US\"")
```

## Expr Class

The `Expr` class provides a type-safe way to build expressions programmatically.

```typescript
import { Expr } from "smql-sdk";
```

### Creating Expressions

```typescript
// Field reference
Expr.field("priority")         // priority

// Literal value
Expr.val(42)                   // 42
Expr.val("hello")              // "hello"
Expr.val(null)                 // NULL
Expr.val(true)                 // true

// State predicates
Expr.stateIs("open")           // STATE IS open
Expr.stateIn("open", "pending") // STATE IN { open, pending }

// Null checks
Expr.isSet("email")            // email IS SET
Expr.isNotSet("phone")         // phone IS NOT SET

// Raw SMQL expression
Expr.raw("STUCK IN open FOR 24h")
```

### Comparisons

```typescript
Expr.field("age").eq(18)       // age == 18
Expr.field("age").neq(0)       // age != 0
Expr.field("age").gt(18)       // age > 18
Expr.field("age").gte(18)      // age >= 18
Expr.field("age").lt(65)       // age < 65
Expr.field("age").lte(65)      // age <= 65

// Compare two fields
Expr.field("actual").gt(Expr.field("expected"))  // actual > expected
```

### Logical Operators

```typescript
const isAdult = Expr.field("age").gte(18);
const isVerified = Expr.field("verified").eq(true);

isAdult.and(isVerified)   // (age >= 18) AND (verified == true)
isAdult.or(isVerified)    // (age >= 18) OR (verified == true)
isAdult.not()             // NOT (age >= 18)
```

### Set Membership

```typescript
Expr.field("status").in("open", "pending", "review")
// status IN { "open", "pending", "review" }
```

### Using with Builders

Pass expressions via `.toString()` or directly in `.where()`:

```typescript
const filter = Expr.field("priority").gte(3)
  .and(Expr.stateIs("open"));

const results = await client.find("Ticket")
  .where(filter.toString())
  .execute();
```

### Complex Example

```typescript
const highPriority = Expr.field("priority").eq(1);
const isOpen = Expr.stateIs("open");
const hasAssignee = Expr.isSet("assignee");

const urgentUnassigned = highPriority
  .and(isOpen)
  .and(hasAssignee.not());

const tickets = await client.find("SupportTicket")
  .where(urgentUnassigned.toString())
  .sortBy("created_at", "ASC")
  .execute();
```
