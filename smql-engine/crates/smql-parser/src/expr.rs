use crate::common;
use crate::lexer::TokenKind;
use crate::Parser;
use smql_ast::expression::*;
use smql_ast::value::Value;
use smql_ast::{SmqlError, SmqlResult};

/// Parse a full expression (entry point for guards and WHERE clauses).
pub fn parse_expression(parser: &mut Parser) -> SmqlResult<Expression> {
    parser.enter_expr()?;
    let result = parse_or_expr(parser);
    parser.leave_expr();
    result
}

/// OR has lowest precedence.
fn parse_or_expr(parser: &mut Parser) -> SmqlResult<Expression> {
    let mut left = parse_and_expr(parser)?;
    while parser.try_keyword("OR") {
        let right = parse_and_expr(parser)?;
        left = Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::Or,
            right: Box::new(right),
        });
    }
    Ok(left)
}

/// AND.
fn parse_and_expr(parser: &mut Parser) -> SmqlResult<Expression> {
    let mut left = parse_not_expr(parser)?;
    while parser.try_keyword("AND") {
        let right = parse_not_expr(parser)?;
        left = Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(left),
            op: BinaryOperator::And,
            right: Box::new(right),
        });
    }
    Ok(left)
}

/// NOT (unary prefix). Recurses into itself so `NOT NOT x` → `NOT(NOT(x))`.
fn parse_not_expr(parser: &mut Parser) -> SmqlResult<Expression> {
    if parser.try_keyword("NOT") {
        let operand = parse_not_expr(parser)?;
        Ok(Expression::new(ExpressionKind::UnaryOp {
            op: UnaryOperator::Not,
            operand: Box::new(operand),
        }))
    } else {
        parse_comparison(parser)
    }
}

/// Comparison operators: ==, !=, <, >, <=, >=, IS SET, IS NOT SET, IN.
fn parse_comparison(parser: &mut Parser) -> SmqlResult<Expression> {
    let left = parse_addition(parser)?;

    // IS SET / IS NOT SET / IS NULL
    if common::check_is_not_set(parser) {
        parser.expect_keyword("IS")?;
        if parser.try_keyword("NOT") {
            parser.expect_keyword("SET")?;
        } else {
            parser.expect_keyword("NULL")?;
        }
        return Ok(Expression::new(ExpressionKind::IsNotSet(Box::new(left))));
    }
    if common::check_is_set(parser) {
        parser.expect_keyword("IS")?;
        parser.expect_keyword("SET")?;
        return Ok(Expression::new(ExpressionKind::IsSet(Box::new(left))));
    }

    // IN { ... } or IN ( ... )
    if parser.check_keyword("IN") {
        parser.expect_keyword("IN")?;
        if parser.check_punct("{") {
            parser.expect_punct("{")?;
            let mut values = Vec::new();
            while !parser.check_punct("}") {
                values.push(parse_expression(parser)?);
                if !parser.try_punct(",") {
                    break;
                }
            }
            parser.expect_punct("}")?;
            return Ok(Expression::new(ExpressionKind::InSet {
                expr: Box::new(left),
                values,
            }));
        } else if parser.check_punct("(") {
            parser.expect_punct("(")?;
            let mut values = Vec::new();
            while !parser.check_punct(")") {
                values.push(parse_expression(parser)?);
                if !parser.try_punct(",") {
                    break;
                }
            }
            parser.expect_punct(")")?;
            return Ok(Expression::new(ExpressionKind::InList {
                expr: Box::new(left),
                values,
            }));
        } else {
            // expr IN collection_expr — dynamic list membership
            // e.g., "approve" IN ACTOR.capabilities
            let collection = parse_postfix(parser)?;
            return Ok(Expression::new(ExpressionKind::InCollection {
                expr: Box::new(left),
                collection: Box::new(collection),
            }));
        }
    }

    // Binary comparison operators
    if let Some(op) = try_parse_comparison_op(parser) {
        let right = parse_addition(parser)?;
        return Ok(Expression::new(ExpressionKind::BinaryOp {
            left: Box::new(left),
            op,
            right: Box::new(right),
        }));
    }

    Ok(left)
}

fn try_parse_comparison_op(parser: &mut Parser) -> Option<BinaryOperator> {
    if parser.try_operator("==") {
        return Some(BinaryOperator::Eq);
    }
    if parser.try_operator("!=") {
        return Some(BinaryOperator::NotEq);
    }
    if parser.try_operator(">=") {
        return Some(BinaryOperator::GtEq);
    }
    if parser.try_operator("<=") {
        return Some(BinaryOperator::LtEq);
    }
    if parser.try_operator(">") {
        return Some(BinaryOperator::Gt);
    }
    if parser.try_operator("<") {
        return Some(BinaryOperator::Lt);
    }
    None
}

/// Addition/subtraction.
fn parse_addition(parser: &mut Parser) -> SmqlResult<Expression> {
    let mut left = parse_multiplication(parser)?;
    loop {
        if parser.try_operator("+") {
            let right = parse_multiplication(parser)?;
            left = Expression::new(ExpressionKind::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::Add,
                right: Box::new(right),
            });
        } else if parser.check_operator("-") {
            // Need to distinguish minus operator from negative number
            // If next token after '-' is a number, it could be subtraction
            parser.advance()?;
            let right = parse_multiplication(parser)?;
            left = Expression::new(ExpressionKind::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::Sub,
                right: Box::new(right),
            });
        } else {
            break;
        }
    }
    Ok(left)
}

/// Multiplication/division.
fn parse_multiplication(parser: &mut Parser) -> SmqlResult<Expression> {
    let mut left = parse_unary(parser)?;
    loop {
        if parser.try_operator("*") {
            let right = parse_unary(parser)?;
            left = Expression::new(ExpressionKind::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::Mul,
                right: Box::new(right),
            });
        } else if parser.try_operator("/") {
            let right = parse_unary(parser)?;
            left = Expression::new(ExpressionKind::BinaryOp {
                left: Box::new(left),
                op: BinaryOperator::Div,
                right: Box::new(right),
            });
        } else {
            break;
        }
    }
    Ok(left)
}

/// Unary prefix: -, NOT (already handled above, so mainly negation).
fn parse_unary(parser: &mut Parser) -> SmqlResult<Expression> {
    parse_postfix(parser)
}

/// Postfix: dot access (a.b.c), and chained calls.
fn parse_postfix(parser: &mut Parser) -> SmqlResult<Expression> {
    let mut expr = parse_primary(parser)?;

    // Handle dot access chains
    while parser.check_punct(".") {
        // Don't consume the dot if this is a float being parsed
        parser.expect_punct(".")?;
        let field = parser.expect_ident()?;

        // If the base is a field access, extend the path
        match &expr.kind {
            ExpressionKind::FieldAccess(path) => {
                let mut new_path = path.clone();
                new_path.push(field);
                expr = Expression::new(ExpressionKind::FieldAccess(new_path));
            }
            ExpressionKind::SelfRef | ExpressionKind::ActorRef => {
                expr = Expression::new(ExpressionKind::QualifiedAccess {
                    root: Box::new(expr),
                    path: vec![field],
                });
            }
            ExpressionKind::QualifiedAccess { root, path } => {
                let mut new_path = path.clone();
                new_path.push(field);
                expr = Expression::new(ExpressionKind::QualifiedAccess {
                    root: root.clone(),
                    path: new_path,
                });
            }
            _ => {
                expr = Expression::new(ExpressionKind::QualifiedAccess {
                    root: Box::new(expr),
                    path: vec![field],
                });
            }
        }
    }

    Ok(expr)
}

/// Primary expressions: literals, identifiers, function calls, parenthesized, keywords.
fn parse_primary(parser: &mut Parser) -> SmqlResult<Expression> {
    // Map/object literal: { key: value, ... }
    if parser.check_punct("{") {
        parser.expect_punct("{")?;
        let mut entries = std::collections::BTreeMap::new();
        while !parser.check_punct("}") {
            let key = parser.expect_ident()?;
            parser.expect_punct(":")?;
            let value = parse_expression(parser)?;
            // For now, store as a nested expression; we'll evaluate to Value::Map at runtime
            entries.insert(key, value);
            parser.try_punct(",");
        }
        parser.expect_punct("}")?;
        // Convert to a Map value with expressions as children
        // For AST purposes, represent as a function-call-like construct
        let args: Vec<Expression> = entries
            .into_iter()
            .flat_map(|(k, v)| vec![Expression::new(ExpressionKind::Literal(Value::Text(k))), v])
            .collect();
        return Ok(Expression::new(ExpressionKind::FunctionCall {
            name: "__map".into(),
            args,
        }));
    }

    // Parenthesized expression
    if parser.check_punct("(") {
        parser.expect_punct("(")?;
        let expr = parse_expression(parser)?;
        parser.expect_punct(")")?;
        return Ok(expr);
    }

    // Duration literal
    if let Some(TokenKind::DurationLiteral(_)) = parser.peek_kind() {
        let d = common::parse_duration(parser)?;
        return Ok(Expression::new(ExpressionKind::DurationLiteral(d)));
    }

    // Numeric and string literals
    if let Some(TokenKind::IntLiteral(_)) = parser.peek_kind() {
        let v = common::parse_literal_value(parser)?;
        return Ok(Expression::new(ExpressionKind::Literal(v)));
    }
    if let Some(TokenKind::FloatLiteral(_)) = parser.peek_kind() {
        let v = common::parse_literal_value(parser)?;
        return Ok(Expression::new(ExpressionKind::Literal(v)));
    }
    if let Some(TokenKind::StringLiteral(_)) = parser.peek_kind() {
        let v = common::parse_literal_value(parser)?;
        return Ok(Expression::new(ExpressionKind::Literal(v)));
    }

    // Keywords that are expressions
    match parser.peek_kind() {
        Some(TokenKind::Keyword(k)) => {
            let kw = k.clone();
            match kw.as_str() {
                "TRUE" => {
                    parser.advance()?;
                    Ok(Expression::new(ExpressionKind::Literal(Value::Bool(true))))
                }
                "FALSE" => {
                    parser.advance()?;
                    Ok(Expression::new(ExpressionKind::Literal(Value::Bool(false))))
                }
                "NULL" => {
                    parser.advance()?;
                    Ok(Expression::new(ExpressionKind::Literal(Value::Null)))
                }
                "SELF" => {
                    parser.advance()?;
                    Ok(Expression::new(ExpressionKind::SelfRef))
                }
                "ACTOR" => {
                    parser.advance()?;
                    Ok(Expression::new(ExpressionKind::ActorRef))
                }
                "STATE" => {
                    parser.advance()?;
                    // STATE IS state_name or STATE IN { ... }
                    if parser.try_keyword("IS") {
                        let state = parser.expect_ident()?;
                        Ok(Expression::new(ExpressionKind::StateIs(state)))
                    } else if parser.try_keyword("IN") {
                        let states = common::parse_ident_set(parser)?;
                        Ok(Expression::new(ExpressionKind::StateIn(states)))
                    } else {
                        // Just "STATE" as a field reference
                        Ok(Expression::new(ExpressionKind::FieldAccess(vec![
                            "STATE".into()
                        ])))
                    }
                }
                "ALL" => {
                    parser.advance()?;
                    parser.expect_punct("(")?;
                    let collection = parse_expression(parser)?;
                    parser.expect_punct(",")?;
                    let predicate = parse_expression(parser)?;
                    parser.expect_punct(")")?;
                    Ok(Expression::new(ExpressionKind::All {
                        collection: Box::new(collection),
                        predicate: Box::new(predicate),
                    }))
                }
                "ANY" => {
                    parser.advance()?;
                    parser.expect_punct("(")?;
                    let collection = parse_expression(parser)?;
                    parser.expect_punct(",")?;
                    let predicate = parse_expression(parser)?;
                    parser.expect_punct(")")?;
                    Ok(Expression::new(ExpressionKind::Any {
                        collection: Box::new(collection),
                        predicate: Box::new(predicate),
                    }))
                }
                "COUNT" => {
                    parser.advance()?;
                    parser.expect_punct("(")?;
                    if parser.check_punct(")") {
                        parser.expect_punct(")")?;
                        Ok(Expression::new(ExpressionKind::Count(None)))
                    } else {
                        let expr = parse_expression(parser)?;
                        parser.expect_punct(")")?;
                        Ok(Expression::new(ExpressionKind::Count(Some(Box::new(expr)))))
                    }
                }
                "SIGNAL" => {
                    parser.advance()?;
                    parser.expect_keyword("FROM")?;
                    let machine = parser.expect_ident()?;
                    parser.expect_keyword("WHERE")?;
                    let condition = parse_expression(parser)?;
                    Ok(Expression::new(ExpressionKind::SignalFrom {
                        machine,
                        condition: Box::new(condition),
                    }))
                }
                "PATTERN" => {
                    parser.advance()?;
                    parser.expect_punct("(")?;
                    let regex = common::parse_string_literal(parser)?;
                    parser.expect_punct(")")?;
                    Ok(Expression::new(ExpressionKind::Pattern(regex)))
                }
                // Query-level predicates
                "ALIVE" => {
                    parser.advance()?;
                    Ok(Expression::new(ExpressionKind::Alive))
                }
                "TERMINATED" => {
                    parser.advance()?;
                    Ok(Expression::new(ExpressionKind::Terminated))
                }
                "STUCK_IN" => {
                    parser.advance()?;
                    parser.expect_punct("(")?;
                    let state = common::parse_string_literal(parser)?;
                    parser.expect_punct(",")?;
                    let duration = common::parse_duration(parser)?;
                    parser.expect_punct(")")?;
                    Ok(Expression::new(ExpressionKind::StuckIn { state, duration }))
                }
                "HAS_VISITED" => {
                    parser.advance()?;
                    parser.expect_punct("(")?;
                    let state = common::parse_string_literal(parser)?;
                    parser.expect_punct(")")?;
                    Ok(Expression::new(ExpressionKind::HasVisited(state)))
                }
                "NEVER_VISITED" => {
                    parser.advance()?;
                    parser.expect_punct("(")?;
                    let state = common::parse_string_literal(parser)?;
                    parser.expect_punct(")")?;
                    Ok(Expression::new(ExpressionKind::NeverVisited(state)))
                }
                // TAG "key" == "value"
                "TAG" => {
                    parser.advance()?;
                    let key = common::parse_string_literal(parser)?;
                    parser.expect_operator("==")?;
                    let value = common::parse_string_literal(parser)?;
                    Ok(Expression::new(ExpressionKind::TagEq { key, value }))
                }
                // Built-in functions (elapsed, NOW, TODAY, etc.)
                "NOW" | "TODAY" => {
                    let name = kw.to_lowercase();
                    parser.advance()?;
                    parser.expect_punct("(")?;
                    parser.expect_punct(")")?;
                    Ok(Expression::new(ExpressionKind::FunctionCall {
                        name,
                        args: vec![],
                    }))
                }
                // Functions that look like keywords
                _ => {
                    // Could be a function call if followed by (
                    // Use original text to preserve case for field names
                    let tok = parser.advance()?;
                    let name = tok.text.clone();
                    if parser.check_punct("(") {
                        parse_function_call_args(parser, name)
                    } else {
                        // Treat as field access / identifier
                        Ok(Expression::new(ExpressionKind::FieldAccess(vec![name])))
                    }
                }
            }
        }
        Some(TokenKind::Identifier(_)) => {
            let tok = parser.advance()?;
            let name = match &tok.kind {
                TokenKind::Identifier(s) => s.clone(),
                _ => unreachable!(),
            };
            // Check for function call
            if parser.check_punct("(") {
                parse_function_call_args(parser, name)
            } else {
                Ok(Expression::new(ExpressionKind::FieldAccess(vec![name])))
            }
        }
        _ => {
            let tok = parser.peek();
            let msg = match tok {
                Some(t) => format!("Expected expression, found '{}'", t.text),
                None => "Expected expression, found end of input".to_string(),
            };
            Err(SmqlError::parse(msg))
        }
    }
}

fn parse_function_call_args(parser: &mut Parser, name: String) -> SmqlResult<Expression> {
    parser.expect_punct("(")?;
    let mut args = Vec::new();
    while !parser.check_punct(")") {
        args.push(parse_expression(parser)?);
        if !parser.try_punct(",") {
            break;
        }
    }
    parser.expect_punct(")")?;
    Ok(Expression::new(ExpressionKind::FunctionCall { name, args }))
}
