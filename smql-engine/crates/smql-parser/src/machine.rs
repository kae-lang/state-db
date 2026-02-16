use crate::common;
use crate::expr;
use crate::lexer::TokenKind;
use crate::Parser;
use smql_ast::machine::*;
use smql_ast::types::*;
use smql_ast::{SmqlError, SmqlResult};

/// Parse DEFINE MACHINE Name ( ... ).
pub fn parse_define_machine(parser: &mut Parser) -> SmqlResult<MachineDefinition> {
    parser.expect_keyword("DEFINE")?;
    parser.expect_keyword("MACHINE")?;
    let name = parser.expect_ident()?;
    parser.expect_punct("(")?;

    let mut machine = MachineDefinition::new(name, String::new());

    // Parse blocks inside the machine definition
    while !parser.check_punct(")") {
        match parser.peek_kind() {
            Some(TokenKind::Keyword(k)) => {
                let kw = k.clone();
                match kw.as_str() {
                    "DATA" => {
                        machine.data = parse_data_block(parser)?;
                    }
                    "STATES" => {
                        machine.states = parse_states_block(parser)?;
                    }
                    "INITIAL" => {
                        machine.initial_state = parse_initial_state(parser)?;
                    }
                    "TERMINAL" => {
                        machine.terminal_states = parse_terminal_states(parser)?;
                    }
                    "TRANSITIONS" => {
                        machine.transitions = parse_transitions_block(parser)?;
                    }
                    "CHILDREN" => {
                        machine.children = parse_children_block(parser)?;
                    }
                    "PARENT" => {
                        machine.parent = Some(parse_parent(parser)?);
                    }
                    "HOOKS" => {
                        machine.hooks = parse_hooks_block(parser)?;
                    }
                    "ROLES" => {
                        machine.roles = parse_roles_block(parser)?;
                    }
                    _ => {
                        return Err(SmqlError::ParseError {
                            message: format!("Unexpected keyword '{}' in machine definition", kw),
                            span: None,
                            hint: Some("Expected: DATA, STATES, INITIAL, TERMINAL, TRANSITIONS, CHILDREN, PARENT, HOOKS, or ROLES".into()),
                        });
                    }
                }
            }
            _ => {
                let tok = parser.peek();
                let msg = match tok {
                    Some(t) => format!("Unexpected token '{}' in machine definition", t.text),
                    None => "Unexpected end of input in machine definition".into(),
                };
                return Err(SmqlError::parse(msg));
            }
        }
    }

    parser.expect_punct(")")?;
    Ok(machine)
}

/// Parse DATA { field : Type -> constraints, ... }.
fn parse_data_block(parser: &mut Parser) -> SmqlResult<Vec<DataFieldDefinition>> {
    parser.expect_keyword("DATA")?;
    parser.expect_punct("{")?;
    let mut fields = Vec::new();
    while !parser.check_punct("}") {
        let field = parse_data_field(parser)?;
        fields.push(field);
    }
    parser.expect_punct("}")?;
    Ok(fields)
}

fn parse_data_field(parser: &mut Parser) -> SmqlResult<DataFieldDefinition> {
    let name = parser.expect_ident()?;
    parser.expect_punct(":")?;
    let field_type = parse_type_definition(parser)?;

    let mut constraints = Vec::new();
    if parser.try_operator("->") {
        constraints = parse_constraints(parser)?;
    }

    Ok(DataFieldDefinition {
        name,
        field_type,
        constraints,
    })
}

fn parse_type_definition(parser: &mut Parser) -> SmqlResult<TypeDefinition> {
    let tok = parser.advance()?;
    match &tok.kind {
        TokenKind::Keyword(k) => {
            match k.as_str() {
                "TEXT" => Ok(TypeDefinition::Text),
                "INT" => Ok(TypeDefinition::Int),
                "FLOAT" => Ok(TypeDefinition::Float),
                "BOOL" => Ok(TypeDefinition::Bool),
                "UUID" => Ok(TypeDefinition::Uuid),
                "DATE" => Ok(TypeDefinition::Date),
                "DATETIME" => Ok(TypeDefinition::DateTime),
                "DURATION" => Ok(TypeDefinition::Duration),
                "BLOB" => Ok(TypeDefinition::Blob),
                "JSON" => Ok(TypeDefinition::Json),
                "ENUM" => {
                    let variants = common::parse_ident_list_parens(parser)?;
                    Ok(TypeDefinition::Enum(variants))
                }
                "REF" => {
                    parser.expect_punct("(")?;
                    let target = parser.expect_ident()?;
                    parser.expect_punct(")")?;
                    Ok(TypeDefinition::Ref(target))
                }
                "LIST" => {
                    parser.expect_punct("(")?;
                    let inner = parse_type_definition(parser)?;
                    parser.expect_punct(")")?;
                    Ok(TypeDefinition::List(Box::new(inner)))
                }
                "SET" => {
                    parser.expect_punct("(")?;
                    let inner = parse_type_definition(parser)?;
                    parser.expect_punct(")")?;
                    Ok(TypeDefinition::Set(Box::new(inner)))
                }
                "MAP" => {
                    parser.expect_punct("(")?;
                    let key = parse_type_definition(parser)?;
                    parser.expect_punct(",")?;
                    let val = parse_type_definition(parser)?;
                    parser.expect_punct(")")?;
                    Ok(TypeDefinition::Map(Box::new(key), Box::new(val)))
                }
                "MONEY" => {
                    parser.expect_punct("(")?;
                    let currency = parser.expect_ident()?;
                    parser.expect_punct(")")?;
                    Ok(TypeDefinition::Money(currency))
                }
                _ => Err(SmqlError::ParseError {
                    message: format!("Unknown type '{}'", k),
                    span: None,
                    hint: Some("Valid types: TEXT, INT, FLOAT, BOOL, UUID, DATE, DATETIME, DURATION, ENUM(...), REF(...), LIST(...), SET(...), MAP(...), MONEY(...), BLOB, JSON".into()),
                }),
            }
        }
        _ => Err(SmqlError::ParseError {
            message: format!("Expected type name, found '{}'", tok.text),
            span: None,
            hint: None,
        }),
    }
}

fn parse_constraints(parser: &mut Parser) -> SmqlResult<Vec<Constraint>> {
    let mut constraints = Vec::new();
    loop {
        let c = parse_single_constraint(parser)?;
        constraints.push(c);
        if !parser.try_punct(",") {
            break;
        }
    }
    Ok(constraints)
}

fn parse_single_constraint(parser: &mut Parser) -> SmqlResult<Constraint> {
    let tok = parser.advance()?;
    match &tok.kind {
        TokenKind::Keyword(k) => {
            match k.as_str() {
                "REQUIRED" => Ok(Constraint::Required),
                "OPTIONAL" => Ok(Constraint::Optional),
                "UNIQUE" => Ok(Constraint::Unique),
                "MAX" => {
                    parser.expect_punct("(")?;
                    let val = common::parse_int_literal(parser)?;
                    parser.expect_punct(")")?;
                    Ok(Constraint::Max(val))
                }
                "MIN" => {
                    parser.expect_punct("(")?;
                    let val = common::parse_int_literal(parser)?;
                    parser.expect_punct(")")?;
                    Ok(Constraint::Min(val))
                }
                "RANGE" => {
                    parser.expect_punct("(")?;
                    let lo = common::parse_int_literal(parser)?;
                    parser.expect_punct(",")?;
                    let hi = common::parse_int_literal(parser)?;
                    parser.expect_punct(")")?;
                    Ok(Constraint::Range(lo, hi))
                }
                "DEFAULT" => {
                    parser.expect_punct("(")?;
                    let dv = parse_default_value(parser)?;
                    parser.expect_punct(")")?;
                    Ok(Constraint::Default(dv))
                }
                "PATTERN" => {
                    parser.expect_punct("(")?;
                    let regex = common::parse_string_literal(parser)?;
                    parser.expect_punct(")")?;
                    Ok(Constraint::Pattern(regex))
                }
                _ => Err(SmqlError::ParseError {
                    message: format!("Unknown constraint '{}'", k),
                    span: None,
                    hint: Some("Valid constraints: REQUIRED, OPTIONAL, MAX(n), MIN(n), RANGE(lo, hi), DEFAULT(val), UNIQUE, PATTERN(regex)".into()),
                }),
            }
        }
        _ => Err(SmqlError::ParseError {
            message: format!("Expected constraint, found '{}'", tok.text),
            span: None,
            hint: None,
        }),
    }
}

fn parse_default_value(parser: &mut Parser) -> SmqlResult<DefaultValue> {
    match parser.peek_kind() {
        Some(TokenKind::StringLiteral(_)) => {
            let s = common::parse_string_literal(parser)?;
            Ok(DefaultValue::String(s))
        }
        Some(TokenKind::IntLiteral(_)) => {
            let v = common::parse_int_literal(parser)?;
            Ok(DefaultValue::Int(v))
        }
        Some(TokenKind::FloatLiteral(v)) => {
            let val = *v;
            parser.advance()?;
            Ok(DefaultValue::Float(val))
        }
        Some(TokenKind::Keyword(k)) => {
            match k.as_str() {
                "TRUE" => {
                    parser.advance()?;
                    Ok(DefaultValue::Bool(true))
                }
                "FALSE" => {
                    parser.advance()?;
                    Ok(DefaultValue::Bool(false))
                }
                "NULL" => {
                    parser.advance()?;
                    Ok(DefaultValue::Null)
                }
                _ => {
                    // Could be an enum variant name
                    let name = parser.expect_ident()?;
                    Ok(DefaultValue::String(name))
                }
            }
        }
        Some(TokenKind::Punctuation(p)) if p == "{" => {
            parser.expect_punct("{")?;
            parser.expect_punct("}")?;
            Ok(DefaultValue::EmptySet)
        }
        Some(TokenKind::Punctuation(p)) if p == "[" => {
            parser.expect_punct("[")?;
            parser.expect_punct("]")?;
            Ok(DefaultValue::EmptyList)
        }
        Some(TokenKind::Identifier(_)) => {
            // Enum variant as identifier
            let name = parser.expect_ident()?;
            Ok(DefaultValue::String(name))
        }
        _ => Err(SmqlError::parse("Expected default value")),
    }
}

/// Parse STATES { state1, state2, ... } or STATES { state1 state2 ... }.
fn parse_states_block(parser: &mut Parser) -> SmqlResult<Vec<StateDefinition>> {
    parser.expect_keyword("STATES")?;
    parser.expect_punct("{")?;
    let mut states = Vec::new();
    while !parser.check_punct("}") {
        let name = parser.expect_ident()?;
        states.push(StateDefinition::new(name));
        // Comma is optional between states
        parser.try_punct(",");
    }
    parser.expect_punct("}")?;
    Ok(states)
}

/// Parse INITIAL STATE state_name.
fn parse_initial_state(parser: &mut Parser) -> SmqlResult<String> {
    parser.expect_keyword("INITIAL")?;
    parser.expect_keyword("STATE")?;
    parser.expect_ident()
}

/// Parse TERMINAL STATES { state1, state2, ... }.
fn parse_terminal_states(parser: &mut Parser) -> SmqlResult<Vec<String>> {
    parser.expect_keyword("TERMINAL")?;
    parser.expect_keyword("STATES")?;
    common::parse_ident_set(parser)
}

/// Parse TRANSITIONS { transition1 { ... } transition2 { ... } ... }.
fn parse_transitions_block(parser: &mut Parser) -> SmqlResult<Vec<TransitionDefinition>> {
    parser.expect_keyword("TRANSITIONS")?;
    parser.expect_punct("{")?;
    let mut transitions = Vec::new();
    while !parser.check_punct("}") {
        let transition = parse_transition_def(parser)?;
        transitions.push(transition);
    }
    parser.expect_punct("}")?;
    Ok(transitions)
}

fn parse_transition_def(parser: &mut Parser) -> SmqlResult<TransitionDefinition> {
    // Parse source: identifier, ANY, or GROUP
    let from = if parser.try_keyword("ANY") {
        let except = Vec::new();
        // The EXCEPT FROM might be inside the body block
        TransitionSource::Any { except }
    } else {
        let state_name = parser.expect_ident()?;
        TransitionSource::State(state_name)
    };

    parser.expect_operator("->")?;
    let to = parser.expect_ident()?;

    parser.expect_punct("{")?;
    let mut transition = TransitionDefinition::new(from, to);

    // Parse the transition body: GUARD, ACTION, TIMEOUT, MUTATE, EXCEPT FROM, SIGNAL
    while !parser.check_punct("}") {
        match parser.peek_kind() {
            Some(TokenKind::Keyword(k)) => {
                let kw = k.clone();
                match kw.as_str() {
                    "GUARD" => {
                        parser.advance()?;
                        parser.expect_punct(":")?;
                        let guard = expr::parse_expression(parser)?;
                        transition.guards.push(guard);
                    }
                    "ACTION" => {
                        parser.advance()?;
                        parser.expect_punct(":")?;
                        let action = parse_action(parser)?;
                        transition.actions.push(action);
                    }
                    "TIMEOUT" => {
                        parser.advance()?;
                        parser.try_punct(":");
                        let duration = common::parse_duration(parser)?;
                        parser.expect_operator("->")?;
                        let target = parser.expect_ident()?;
                        transition.timeout = Some(TimeoutClause {
                            duration,
                            target_state: target,
                        });
                    }
                    "MUTATE" => {
                        parser.advance()?;
                        parser.expect_punct(":")?;
                        let field = parser.expect_ident()?;
                        parser.expect_operator("=")?;
                        // Check for SPAWN as a special construct in MUTATE context
                        if parser.check_keyword("SPAWN") {
                            parser.advance()?;
                            let machine = parser.expect_ident()?;
                            let mut data = Vec::new();
                            if parser.check_punct("{") {
                                parser.expect_punct("{")?;
                                while !parser.check_punct("}") {
                                    let k = parser.expect_ident()?;
                                    parser.expect_punct(":")?;
                                    let v = expr::parse_expression(parser)?;
                                    data.push((k, v));
                                    parser.try_punct(",");
                                }
                                parser.expect_punct("}")?;
                            }
                            // Represent as a function call: __spawn(machine, key1, val1, ...)
                            let mut args = vec![smql_ast::expression::Expression::new(
                                smql_ast::expression::ExpressionKind::Literal(
                                    smql_ast::value::Value::Text(machine),
                                ),
                            )];
                            for (k, v) in data {
                                args.push(smql_ast::expression::Expression::new(
                                    smql_ast::expression::ExpressionKind::Literal(
                                        smql_ast::value::Value::Text(k),
                                    ),
                                ));
                                args.push(v);
                            }
                            let value = smql_ast::expression::Expression::new(
                                smql_ast::expression::ExpressionKind::FunctionCall {
                                    name: "__spawn".into(),
                                    args,
                                },
                            );
                            transition.mutates.push(MutateClause { field, value });
                        } else {
                            let value = expr::parse_expression(parser)?;
                            transition.mutates.push(MutateClause { field, value });
                        }
                    }
                    "EXCEPT" => {
                        parser.advance()?;
                        parser.expect_keyword("FROM")?;
                        let except = common::parse_ident_set(parser)?;
                        // Update the from to include exceptions
                        if let TransitionSource::Any { except: ref mut e } = transition.from {
                            *e = except;
                        }
                    }
                    "SIGNAL" => {
                        parser.advance()?;
                        parser.expect_keyword("PARENT")?;
                        parser.expect_keyword("TO")?;
                        let target_state = parser.expect_ident()?;
                        transition
                            .actions
                            .push(Action::SignalParent { target_state });
                    }
                    _ => {
                        // Skip unknown tokens in transition body
                        parser.advance()?;
                    }
                }
            }
            _ => {
                parser.advance()?;
            }
        }
    }

    parser.expect_punct("}")?;
    Ok(transition)
}

fn parse_action(parser: &mut Parser) -> SmqlResult<Action> {
    match parser.peek_kind() {
        Some(TokenKind::Keyword(k)) => {
            let kw = k.clone();
            match kw.as_str() {
                "NOTIFY" => {
                    parser.advance()?;
                    parser.expect_punct("(")?;
                    let target = expr::parse_expression(parser)?;
                    parser.expect_punct(",")?;
                    let event = common::parse_string_literal(parser)?;
                    parser.expect_punct(")")?;
                    Ok(Action::Notify { target, event })
                }
                "LOG" => {
                    parser.advance()?;
                    parser.expect_punct("(")?;
                    let msg = common::parse_string_literal(parser)?;
                    parser.expect_punct(")")?;
                    Ok(Action::Log(msg))
                }
                "EMIT" => {
                    parser.advance()?;
                    parser.expect_punct("(")?;
                    let event = common::parse_string_literal(parser)?;
                    let payload = if parser.try_punct(",") {
                        Some(expr::parse_expression(parser)?)
                    } else {
                        None
                    };
                    parser.expect_punct(")")?;
                    Ok(Action::Emit { event, payload })
                }
                "WEBHOOK" => {
                    parser.advance()?;
                    parser.expect_punct("(")?;
                    let url = common::parse_string_literal(parser)?;
                    let payload = if parser.try_punct(",") {
                        Some(expr::parse_expression(parser)?)
                    } else {
                        None
                    };
                    parser.expect_punct(")")?;
                    Ok(Action::Webhook { url, payload })
                }
                "SPAWN" => {
                    parser.advance()?;
                    let machine = parser.expect_ident()?;
                    let mut data = Vec::new();
                    if parser.check_punct("{") {
                        parser.expect_punct("{")?;
                        while !parser.check_punct("}") {
                            let field = parser.expect_ident()?;
                            parser.expect_punct(":")?;
                            let value = expr::parse_expression(parser)?;
                            data.push((field, value));
                            parser.try_punct(",");
                        }
                        parser.expect_punct("}")?;
                    }
                    Ok(Action::SpawnChild { machine, data })
                }
                "SIGNAL" => {
                    parser.advance()?;
                    parser.expect_keyword("PARENT")?;
                    parser.expect_keyword("TO")?;
                    let target_state = parser.expect_ident()?;
                    Ok(Action::SignalParent { target_state })
                }
                _ => Err(SmqlError::ParseError {
                    message: format!("Unknown action '{}'", kw),
                    span: None,
                    hint: Some(
                        "Valid actions: NOTIFY, LOG, EMIT, WEBHOOK, SPAWN, SIGNAL PARENT TO".into(),
                    ),
                }),
            }
        }
        _ => Err(SmqlError::parse("Expected action keyword")),
    }
}

/// Parse CHILDREN { name : LIST(Machine) -> constraints, ... }.
fn parse_children_block(parser: &mut Parser) -> SmqlResult<Vec<ChildDefinition>> {
    parser.expect_keyword("CHILDREN")?;
    parser.expect_punct("{")?;
    let mut children = Vec::new();
    while !parser.check_punct("}") {
        let name = parser.expect_ident()?;
        parser.expect_punct(":")?;

        let (machine, cardinality) = parse_child_type(parser)?;
        let mut child = ChildDefinition {
            name,
            machine,
            cardinality,
        };

        // Optional constraints after ->
        if parser.try_operator("->") {
            // Parse constraints like MIN(1)
            while let Some(TokenKind::Keyword(k)) = parser.peek_kind() {
                match k.as_str() {
                    "MIN" => {
                        parser.advance()?;
                        parser.expect_punct("(")?;
                        let min = common::parse_int_literal(parser)?;
                        parser.expect_punct(")")?;
                        if let ChildCardinality::List {
                            min: ref mut min_val,
                            ..
                        } = child.cardinality
                        {
                            *min_val = Some(min as u64);
                        }
                    }
                    "MAX" => {
                        parser.advance()?;
                        parser.expect_punct("(")?;
                        let max = common::parse_int_literal(parser)?;
                        parser.expect_punct(")")?;
                        if let ChildCardinality::List {
                            max: ref mut max_val,
                            ..
                        } = child.cardinality
                        {
                            *max_val = Some(max as u64);
                        }
                    }
                    _ => break,
                }
                if !parser.try_punct(",") {
                    break;
                }
            }
        }

        children.push(child);
    }
    parser.expect_punct("}")?;
    Ok(children)
}

fn parse_child_type(parser: &mut Parser) -> SmqlResult<(String, ChildCardinality)> {
    let tok = parser.advance()?;
    match &tok.kind {
        TokenKind::Keyword(k) => match k.as_str() {
            "LIST" => {
                parser.expect_punct("(")?;
                let machine = parser.expect_ident()?;
                parser.expect_punct(")")?;
                Ok((
                    machine,
                    ChildCardinality::List {
                        min: None,
                        max: None,
                    },
                ))
            }
            "OPTIONAL" => {
                parser.expect_punct("(")?;
                let machine = parser.expect_ident()?;
                parser.expect_punct(")")?;
                Ok((machine, ChildCardinality::Optional))
            }
            _ => Err(SmqlError::ParseError {
                message: format!("Expected LIST or OPTIONAL, found '{}'", k),
                span: None,
                hint: None,
            }),
        },
        TokenKind::Identifier(name) => Ok((name.clone(), ChildCardinality::Required)),
        _ => Err(SmqlError::parse("Expected child type")),
    }
}

/// Parse PARENT : MachineName.
fn parse_parent(parser: &mut Parser) -> SmqlResult<String> {
    parser.expect_keyword("PARENT")?;
    parser.expect_punct(":")?;
    parser.expect_ident()
}

/// Parse HOOKS { ... } block.
fn parse_hooks_block(parser: &mut Parser) -> SmqlResult<Vec<HookDefinition>> {
    parser.expect_keyword("HOOKS")?;
    parser.expect_punct("{")?;
    let mut hooks = Vec::new();
    while !parser.check_punct("}") {
        let trigger = parse_hook_trigger(parser)?;
        parser.expect_punct("{")?;
        let mut actions = Vec::new();
        while !parser.check_punct("}") {
            let kw = parser.peek_kind();
            if matches!(kw, Some(TokenKind::Keyword(k)) if k == "ACTION") {
                parser.advance()?;
                parser.expect_punct(":")?;
            }
            let action = parse_action(parser)?;
            actions.push(action);
        }
        parser.expect_punct("}")?;
        hooks.push(HookDefinition { trigger, actions });
    }
    parser.expect_punct("}")?;
    Ok(hooks)
}

fn parse_hook_trigger(parser: &mut Parser) -> SmqlResult<HookTrigger> {
    if parser.try_keyword("ON") {
        if parser.try_keyword("SPAWN") {
            Ok(HookTrigger::OnSpawn)
        } else if parser.try_keyword("ENTER") {
            let state = parser.expect_ident()?;
            Ok(HookTrigger::OnEnter(state))
        } else if parser.try_keyword("EXIT") {
            let state = parser.expect_ident()?;
            Ok(HookTrigger::OnExit(state))
        } else if parser.try_keyword("DWELL") {
            parser.expect_punct("(")?;
            let state = parser.expect_ident()?;
            parser.expect_punct(",")?;
            parser.expect_operator(">")?;
            let duration = common::parse_duration(parser)?;
            parser.expect_punct(")")?;
            Ok(HookTrigger::OnDwell { state, duration })
        } else {
            Err(SmqlError::parse(
                "Expected SPAWN, ENTER, EXIT, or DWELL after ON",
            ))
        }
    } else if parser.try_keyword("BEFORE") {
        parser.expect_keyword("EACH")?;
        parser.expect_keyword("TRANSITION")?;
        Ok(HookTrigger::BeforeEachTransition)
    } else if parser.try_keyword("AFTER") {
        parser.expect_keyword("EACH")?;
        parser.expect_keyword("TRANSITION")?;
        Ok(HookTrigger::AfterEachTransition)
    } else {
        Err(SmqlError::parse(
            "Expected ON, BEFORE, or AFTER for hook trigger",
        ))
    }
}

/// Parse ROLES { ... } block.
fn parse_roles_block(parser: &mut Parser) -> SmqlResult<Vec<RoleDefinition>> {
    parser.expect_keyword("ROLES")?;
    parser.expect_punct("{")?;
    let mut roles = Vec::new();
    while !parser.check_punct("}") {
        let name = parser.expect_ident()?;
        parser.expect_punct("{")?;
        let mut permissions = Vec::new();
        while !parser.check_punct("}") {
            // Parse permissions like CAN SPAWN, CAN TRANSITION [states], CAN QUERY
            parser.expect_keyword("CAN")?;
            let perm_kw = parser.expect_ident()?;
            match perm_kw.to_uppercase().as_str() {
                "SPAWN" => permissions.push(RolePermission::CanSpawn),
                "TRANSITION" => {
                    if parser.check_punct("[") {
                        parser.expect_punct("[")?;
                        let mut states = Vec::new();
                        while !parser.check_punct("]") {
                            states.push(parser.expect_ident()?);
                            parser.try_punct(",");
                        }
                        parser.expect_punct("]")?;
                        permissions.push(RolePermission::CanTransition(states));
                    } else {
                        permissions.push(RolePermission::CanTransition(vec![]));
                    }
                }
                "QUERY" => permissions.push(RolePermission::CanQuery),
                "ALTER" => permissions.push(RolePermission::CanAlter),
                _ => {
                    return Err(SmqlError::parse(format!(
                        "Unknown permission '{}'",
                        perm_kw
                    )));
                }
            }
        }
        parser.expect_punct("}")?;
        roles.push(RoleDefinition { name, permissions });
    }
    parser.expect_punct("}")?;
    Ok(roles)
}

/// Public helper for ALTER MACHINE ADD DATA.
pub fn parse_data_field_from_alter(parser: &mut Parser) -> SmqlResult<DataFieldDefinition> {
    parse_data_field(parser)
}
