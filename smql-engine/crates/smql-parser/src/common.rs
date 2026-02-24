use crate::lexer::TokenKind;
use crate::Parser;
use smql_ast::value::{SmqlDuration, Value};
use smql_ast::{SmqlError, SmqlResult, Span};

/// Parse a string literal, returning its contents.
pub fn parse_string_literal(parser: &mut Parser) -> SmqlResult<String> {
    let tok = parser.advance()?;
    match &tok.kind {
        TokenKind::StringLiteral(s) => Ok(s.clone()),
        _ => Err(SmqlError::ParseError {
            message: format!("Expected string literal, found '{}'", tok.text),
            span: Some(Span::new(tok.offset, tok.offset + tok.text.len())),
            hint: None,
        }),
    }
}

/// Parse an integer literal.
pub fn parse_int_literal(parser: &mut Parser) -> SmqlResult<i64> {
    let tok = parser.advance()?;
    match &tok.kind {
        TokenKind::IntLiteral(v) => Ok(*v),
        _ => Err(SmqlError::ParseError {
            message: format!("Expected integer, found '{}'", tok.text),
            span: Some(Span::new(tok.offset, tok.offset + tok.text.len())),
            hint: None,
        }),
    }
}

/// Parse a non-negative integer literal and return as u64.
/// Rejects negative values instead of silently wrapping via `as u64`.
pub fn parse_non_negative_int(parser: &mut Parser) -> SmqlResult<u64> {
    let tok = parser.peek().cloned();
    let val = parse_int_literal(parser)?;
    if val < 0 {
        let span = tok.map(|t| Span::new(t.offset, t.offset + t.text.len()));
        return Err(SmqlError::ParseError {
            message: format!("Expected non-negative integer, found {}", val),
            span,
            hint: Some("LIMIT, OFFSET, and INTERVAL values must be >= 0".into()),
        });
    }
    Ok(val as u64)
}

/// Parse a duration literal, returning seconds.
pub fn parse_duration(parser: &mut Parser) -> SmqlResult<SmqlDuration> {
    let tok = parser.advance()?;
    match &tok.kind {
        TokenKind::DurationLiteral(secs) => Ok(SmqlDuration::from_seconds(*secs)),
        _ => Err(SmqlError::ParseError {
            message: format!("Expected duration (e.g. 24h, 7d), found '{}'", tok.text),
            span: Some(Span::new(tok.offset, tok.offset + tok.text.len())),
            hint: Some(
                "Durations use suffixes: s (seconds), m (minutes), h (hours), d (days)".into(),
            ),
        }),
    }
}

/// Parse a literal value (string, int, float, bool, null).
pub fn parse_literal_value(parser: &mut Parser) -> SmqlResult<Value> {
    match parser.peek_kind() {
        Some(TokenKind::StringLiteral(_)) => {
            let s = parse_string_literal(parser)?;
            Ok(Value::Text(s))
        }
        Some(TokenKind::IntLiteral(_)) => {
            let v = parse_int_literal(parser)?;
            Ok(Value::Int(v))
        }
        Some(TokenKind::FloatLiteral(_)) => {
            let tok = parser.advance()?;
            if let TokenKind::FloatLiteral(v) = tok.kind {
                Ok(Value::Float(v))
            } else {
                unreachable!()
            }
        }
        Some(TokenKind::Keyword(k)) if k == "TRUE" => {
            parser.advance()?;
            Ok(Value::Bool(true))
        }
        Some(TokenKind::Keyword(k)) if k == "FALSE" => {
            parser.advance()?;
            Ok(Value::Bool(false))
        }
        Some(TokenKind::Keyword(k)) if k == "NULL" => {
            parser.advance()?;
            Ok(Value::Null)
        }
        _ => {
            let tok = parser.peek();
            let msg = match tok {
                Some(t) => format!("Expected literal value, found '{}'", t.text),
                None => "Expected literal value, found end of input".to_string(),
            };
            Err(SmqlError::parse(msg))
        }
    }
}

/// Parse a comma-separated list of identifiers within braces: { a, b, c }.
pub fn parse_ident_set(parser: &mut Parser) -> SmqlResult<Vec<String>> {
    parser.expect_punct("{")?;
    let mut items = Vec::new();
    while !parser.check_punct("}") {
        let name = parser.expect_ident()?;
        items.push(name);
        if !parser.try_punct(",") {
            break;
        }
    }
    parser.expect_punct("}")?;
    Ok(items)
}

/// Parse a comma-separated list of identifiers in parens: ( a, b, c ) — returns strings.
pub fn parse_ident_list_parens(parser: &mut Parser) -> SmqlResult<Vec<String>> {
    parser.expect_punct("(")?;
    let mut items = Vec::new();
    while !parser.check_punct(")") {
        // Could be identifier or string
        match parser.peek_kind() {
            Some(TokenKind::StringLiteral(_)) => {
                let s = parse_string_literal(parser)?;
                items.push(s);
            }
            _ => {
                let name = parser.expect_ident()?;
                items.push(name);
            }
        }
        if !parser.try_punct(",") {
            break;
        }
    }
    parser.expect_punct(")")?;
    Ok(items)
}

/// Check if the next token sequence is `IS SET`.
pub fn check_is_set(parser: &Parser) -> bool {
    if let Some(TokenKind::Keyword(k)) = parser.peek_kind() {
        if k == "IS" {
            if let Some(t2) = parser.peek_nth(1) {
                if let TokenKind::Keyword(k2) = &t2.kind {
                    return k2 == "SET";
                }
            }
        }
    }
    false
}

/// Check if the next token sequence is `IS NOT SET` or `IS NULL`.
pub fn check_is_not_set(parser: &Parser) -> bool {
    if let Some(TokenKind::Keyword(k)) = parser.peek_kind() {
        if k == "IS" {
            if let Some(t2) = parser.peek_nth(1) {
                if let TokenKind::Keyword(k2) = &t2.kind {
                    if k2 == "NOT" {
                        if let Some(t3) = parser.peek_nth(2) {
                            if let TokenKind::Keyword(k3) = &t3.kind {
                                return k3 == "SET";
                            }
                        }
                    } else if k2 == "NULL" {
                        return true;
                    }
                }
            }
        }
    }
    false
}
