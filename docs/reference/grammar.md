# Grammar

A simplified EBNF grammar for the SMQL language.

## Conventions

- `UPPER_CASE` — keywords (case-insensitive)
- `lower_case` — grammar rules
- `"..."` — literal tokens
- `[ ... ]` — optional
- `{ ... }` — zero or more repetitions
- `( ... | ... )` — alternatives

## Top-Level

```txt
program         = { statement } ;
statement       = command | query ;
command         = define_machine | spawn | transition | try_transition
                | transition_all | alter_machine ;
query           = get | find | aggregate | trail | paths | funnel | compare_paths ;
```

## Machine Definition

```txt
define_machine  = DEFINE MACHINE ident "(" machine_body ")" ;

machine_body    = { machine_clause } ;
machine_clause  = data_block | states_block | initial_state
                | terminal_states | children_block | parent_decl
                | transitions_block | hooks_block ;

data_block      = DATA "{" { data_field } "}" ;
data_field      = ident ":" type_expr [ "->" constraints ] ;

type_expr       = TEXT | INT | FLOAT | BOOL | UUID | DATE | DATETIME
                | DURATION | MONEY "(" ident ")" | BLOB | JSON
                | ENUM "(" ident { "," ident } ")"
                | REF "(" ident ")"
                | LIST "(" type_expr ")"
                | SET "(" type_expr ")"
                | MAP "(" type_expr "," type_expr ")" ;

constraints     = constraint { "," constraint } ;
constraint      = REQUIRED | OPTIONAL
                | DEFAULT "(" literal ")"
                | MIN "(" number ")"
                | MAX "(" number ")"
                | RANGE "(" number "," number ")"
                | UNIQUE
                | PATTERN "(" string ")" ;

states_block    = STATES "{" ident { "," ident } "}" ;
initial_state   = INITIAL STATE ident ;
terminal_states = TERMINAL STATES "{" ident { "," ident } "}" ;

children_block  = CHILDREN "{" { child_field } "}" ;
child_field     = ident ":" child_type [ "->" constraints ] ;
child_type      = LIST "(" ident ")" | OPTIONAL "(" ident ")" ;

parent_decl     = PARENT ":" ident ;
```

## Transitions

```txt
transitions_block = TRANSITIONS "{" { transition_def } "}" ;
transition_def    = transition_source "->" ident "{" { transition_clause } "}" ;
transition_source = ident | ANY ;

transition_clause = guard_clause | action_clause | timeout_clause
                  | mutate_clause | except_clause ;
guard_clause      = GUARD ":" expression ;
action_clause     = ACTION ":" action ;
timeout_clause    = TIMEOUT ":" duration "->" ident ;
mutate_clause     = MUTATE ":" ident "=" expression ;
except_clause     = EXCEPT FROM "{" ident { "," ident } "}" ;

action            = LOG "(" string ")"
                  | NOTIFY "(" expression "," string ")"
                  | EMIT "(" string [ "," expression ] ")"
                  | WEBHOOK "(" string [ "," expression ] ")" ;
```

## Hooks

```txt
hooks_block     = HOOKS "{" { hook_def } "}" ;
hook_def        = ON SPAWN "{" { action } "}"
                | BEFORE EACH TRANSITION "{" { action } "}"
                | AFTER EACH TRANSITION "{" { action } "}"
                | ON ENTER ident "{" { action } "}"
                | ON EXIT ident "{" { action } "}" ;
```

## Commands

```txt
spawn           = SPAWN ident "{" [ data_pairs ] "}"
                  [ THEN TRANSITION TO ident ] ;
data_pairs      = data_pair { "," data_pair } ;
data_pair       = ident ":" expression ;

transition      = TRANSITION ident string TO ident
                  [ AS string ]
                  [ WITH "{" data_pairs "}" ]
                  [ MEMO string ]
                  [ THROUGH "[" ident { "," ident } "]" ]
                  [ OR_STAY ]
                  [ CASCADE ] ;

try_transition  = TRY TRANSITION ident string TO ident
                  [ AS string ]
                  [ WITH "{" data_pairs "}" ]
                  [ MEMO string ] ;

transition_all  = TRANSITION ALL ident WHERE expression TO ident ;

alter_machine   = ALTER MACHINE ident { alter_op } ;
alter_op        = ADD STATE ident
                | REMOVE STATE ident MIGRATE TO ident
                | ADD TRANSITION ident "->" ident
                | REMOVE TRANSITION ident "->" ident
                | ADD DATA data_field
                | REMOVE DATA ident
                | BACKFILL ident "=" literal ;
```

## Queries

```txt
get             = GET ident string ;

find            = FIND ident
                  [ WHERE expression ]
                  [ SORT [ BY ] sort_clause { "," sort_clause } ]
                  [ LIMIT number ]
                  [ OFFSET number ]
                  [ AFTER string ] ;
sort_clause     = ident ( ASC | DESC ) ;

aggregate       = AGGREGATE ident
                  MEASURE measure_def { "," measure_def }
                  [ WHERE expression ]
                  [ GROUP BY ( STATE | ident { "," ident } ) ] ;
measure_def     = agg_func [ AS ident ] ;
agg_func        = COUNT "(" ")"
                | ( SUM | AVG | MIN | MAX ) "(" ident ")"
                | PERCENTILE "(" ident ")" ;

trail           = TRAIL OF string [ WHERE expression ] ;

paths           = PATHS FROM ident [ WHERE expression ] [ LIMIT number ] ;

funnel          = FUNNEL ident THROUGH "[" ident { "," ident } "]"
                  [ WHERE expression ] ;

compare_paths   = COMPARE PATHS ident SEGMENT BY ident
                  [ WHERE expression ] ;
```

## Expressions

```txt
expression      = or_expr ;
or_expr         = and_expr { OR and_expr } ;
and_expr        = not_expr { AND not_expr } ;
not_expr        = NOT not_expr | comparison ;
comparison      = addition [ comp_op addition ]
                | addition IS SET
                | addition IS NOT SET
                | addition IN "(" expression { "," expression } ")" ;
comp_op         = "==" | "!=" | ">" | "<" | ">=" | "<=" ;
addition        = multiplication { ( "+" | "-" ) multiplication } ;
multiplication  = unary { ( "*" | "/" ) unary } ;
unary           = [ "-" ] primary ;

primary         = literal | ident [ "." ident ]
                | function_call
                | "(" expression ")"
                | "{" data_pairs "}"
                | ALL "(" ident "," expression ")"
                | ANY "(" ident "," expression ")"
                | STATE IS ident
                | STATE IN "{" ident { "," ident } "}"
                | ALIVE | TERMINATED
                | SELF | ACTOR [ "." ident ] ;

function_call   = ident "(" [ expression { "," expression } ] ")" ;

literal         = string | number | float | TRUE | FALSE | NULL | duration ;
string          = "\"" { char } "\"" ;
number          = digit { digit } ;
float           = number "." number ;
duration        = number ( "s" | "m" | "h" | "d" ) ;
ident           = letter { letter | digit | "_" } ;
```

::: info
This grammar is simplified for readability. The actual parser handles additional edge cases, error recovery, and comment stripping (`--` line comments).
:::
