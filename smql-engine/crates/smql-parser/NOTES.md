# smql-parser — Session Notes

## 2026-02-15 — Initial Implementation

### What was done
- Full SMQL parser: lexer, expression parser, machine definition parser, command parsers, query parsers
- 62 tests covering lexer tokenization, expression parsing, machine definitions, commands, and queries
- Successfully parses support_ticket.smql and order.smql end-to-end

### Architecture
- Hand-written recursive descent parser (not using winnow for parsing — it's cleaner)
- Lexer produces Token stream, Parser walks tokens
- Expression parser uses precedence climbing: OR < AND < NOT < comparison < addition < multiplication < unary < postfix < primary

### Design decisions
- Keywords stored as UPPERCASED in TokenKind::Keyword, but `expect_ident()` returns original text (preserves case for field names)
- Dot (`.`) is a punctuation token, handled in expression postfix for field access chains
- Map literals `{ key: value }` parsed as `__map` function calls in the AST
- `SPAWN` in MUTATE context parsed as `__spawn` function call (not a regular expression)
- EXCEPT FROM clause in wildcard transitions is parsed inside the transition body, not in the transition header

### Known limitations
- Error recovery is basic (stops at first error, no skip-and-continue)
- No fuzzy suggestion for typos yet (2.7.2 deferred)
- Duration parsing only handles single-unit durations (e.g. `24h`, not `1d12h`)
